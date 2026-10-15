//! Struct-literal field names against the DECLARED struct (issue #152, slice 2).
//!
//! `hyb check` resolved names, but never related an expression to the declared
//! shape it must match. A struct literal is the cheapest case to close: the
//! literal states its own type, so the field names can be compared with no
//! inference at all.
//!
//! ```hy
//! struct titik {
//!     x int
//!     y int
//! }
//!
//! fn main() {
//!     p := titik{x: 1, z: 2}
//!     fmt.println("{p.x}")
//! }
//! ```
//!
//! ```text
//! $ hyb check field_hantu.hy   -> field_hantu.hy: OK (0 diagnostics)
//! $ go build ./...             -> ./x.go:13:19: unknown field z in struct literal of type titik
//! ```
//!
//! The same holds for a field given twice (Go: `duplicate field x in struct
//! literal`). Both are certain and local.
//!
//! Scope is deliberately narrow, because a false reject is far worse than a
//! missed one: the literal must name its struct with a **bare, non-dotted**
//! identifier, and that struct must be declared in the same file. Qualified
//! (`pkg.thing{...}`) and type-elided (`{x: 1}`) literals are skipped, and a
//! **partial** field set is silent — Go allows naming a subset.

use hy_syntax::{Expr, FieldInit, LitKind, TypeExpr};

/// Why a struct literal does not match its declared struct.
pub enum FieldError {
    /// A field name the struct does not declare.
    Unknown { name: String },
    /// The same field given more than once (Go: "duplicate field").
    Duplicate { name: String },
}

/// The struct name of a struct literal, when the literal names it with a
/// **bare** identifier (`titik{x: 1}`). Dotted type paths (`pkg.thing{...}`)
/// are foreign and type-elided literals carry no name; both return `None`.
pub fn bare_struct_lit_type(ty: &Option<TypeExpr>) -> Option<&str> {
    match ty.as_ref()? {
        TypeExpr::Named { name, .. } if !name.contains('.') => Some(name.as_str()),
        _ => None,
    }
}

/// Check a struct literal's field names against the DECLARED fields.
///
/// `declared` is the struct's field list as `(name, type)` pairs. Returns the
/// first problem in source order, or `None` when the literal is consistent.
/// An empty `declared` list means the struct is unknown here (zero-field alias
/// declarations are never registered as containers) and is never reported.
pub fn check_struct_lit_fields(
    declared: &[(String, TypeExpr)],
    fields: &[FieldInit],
) -> Option<FieldError> {
    if declared.is_empty() {
        return None;
    }
    let mut seen: Vec<&str> = Vec::new();
    for f in fields {
        if declared.iter().any(|(n, _)| n == &f.name) {
            if seen.contains(&f.name.as_str()) {
                return Some(FieldError::Duplicate {
                    name: f.name.clone(),
                });
            }
            seen.push(f.name.as_str());
        } else {
            return Some(FieldError::Unknown {
                name: f.name.clone(),
            });
        }
    }
    None
}

/// `a, b, c` — the declared field names in order, for a diagnostic note.
pub fn render_field_names(declared: &[(String, TypeExpr)]) -> String {
    declared
        .iter()
        .map(|(n, _)| n.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Why a struct-literal field VALUE cannot be the declared type.
pub enum ValueError {
    /// A literal of a family the declared field type can never hold.
    Mismatch {
        field: String,
        declared: String,
        given: &'static str,
    },
}

/// The primitive families this pass understands well enough to be certain.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Family {
    Int,
    Float,
    Str,
    Bool,
}

fn primitive_family(name: &str) -> Option<Family> {
    match name {
        "int" | "int8" | "int16" | "int32" | "int64" | "uint" | "uint8" | "uint16" | "uint32"
        | "uint64" | "u8" | "u16" | "u32" | "u64" | "i8" | "i16" | "i32" | "i64" => {
            Some(Family::Int)
        }
        "float32" | "float64" | "f32" | "f64" => Some(Family::Float),
        "string" => Some(Family::Str),
        "bool" => Some(Family::Bool),
        _ => None,
    }
}

/// What a value literal certainly IS, at the granularity this pass needs.
/// Anything that is not a literal (an identifier, a call) returns `None`:
/// its type is not knowable here, so it is never reported.
fn literal_class(v: &Expr) -> Option<(Family, &'static str)> {
    match v {
        Expr::Lit {
            kind: LitKind::Int(_),
            ..
        } => Some((Family::Int, "an integer literal")),
        Expr::Lit {
            kind: LitKind::Float(_),
            ..
        } => Some((Family::Float, "a float literal")),
        Expr::Lit {
            kind: LitKind::Str(_) | LitKind::RawStr(_),
            ..
        } => Some((Family::Str, "a string literal")),
        Expr::Lit {
            kind: LitKind::Bool(_),
            ..
        } => Some((Family::Bool, "a bool literal")),
        // An interpolated string is a Go string; its type is never in doubt.
        Expr::Interp { .. } => Some((Family::Str, "an interpolated string")),
        _ => None,
    }
}

/// Can a literal of family `given` ever be the declared family?
/// An integer literal is fine for a float field (Go converts the constant);
/// nothing else crosses.
fn compatible(declared: Family, given: Family) -> bool {
    match declared {
        Family::Int => given == Family::Int,
        Family::Float => given == Family::Int || given == Family::Float,
        Family::Str => given == Family::Str,
        Family::Bool => given == Family::Bool,
    }
}

/// Check struct-literal field VALUES against the declared field types.
///
/// Only certain, local mismatches are reported: the field must be declared,
/// its declared type must be a built-in primitive this pass knows, and the
/// value must be a literal (or an interpolated string). Everything else —
/// identifiers, calls, user types, slices, maps, `any` — is silent.
pub fn check_field_value_literals(
    declared: &[(String, TypeExpr)],
    fields: &[FieldInit],
) -> Option<ValueError> {
    if declared.is_empty() {
        return None;
    }
    for f in fields {
        let Some((_, ty)) = declared.iter().find(|(n, _)| n == &f.name) else {
            continue; // an unknown field name is E5004's business, not this pass
        };
        let TypeExpr::Named { name, .. } = ty else {
            continue;
        };
        let Some(want) = primitive_family(name) else {
            continue;
        };
        let Some((got, desc)) = literal_class(&f.value) else {
            continue;
        };
        if !compatible(want, got) {
            return Some(ValueError::Mismatch {
                field: f.name.clone(),
                declared: name.clone(),
                given: desc,
            });
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use hy_syntax::{Decl, Expr, Stmt};

    fn struct_fields(src: &str) -> Vec<(String, TypeExpr)> {
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        for d in &e.decls {
            if let Decl::Struct(s) = d {
                return s.fields.iter().map(|f| (f.name.clone(), f.ty.clone())).collect();
            }
        }
        panic!("no struct decl")
    }

    fn lit_fields(src: &str) -> (Option<TypeExpr>, Vec<FieldInit>) {
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        let f = e.fns().next().unwrap();
        let stmt = &f.body.as_ref().unwrap().stmts[0];
        if let Stmt::Local(l) = stmt {
            if let Expr::StructLit { ty, fields, .. } = &l.values[0] {
                return (ty.clone(), fields.clone());
            }
        }
        panic!("expected a struct literal local")
    }

    const TITIK: &str = "struct titik {\n    x int\n    y int\n}\nfn main() { p := titik{x: 1, y: 2} }";

    #[test]
    fn unknown_field_in_struct_literal_is_reported() {
        let d = struct_fields(TITIK);
        let (ty, f) = lit_fields(
            "struct titik {\n    x int\n    y int\n}\nfn main() { p := titik{x: 1, z: 2} }",
        );
        assert_eq!(bare_struct_lit_type(&ty), Some("titik"));
        match check_struct_lit_fields(&d, &f) {
            Some(FieldError::Unknown { name }) => assert_eq!(name, "z"),
            _ => panic!("expected Unknown"),
        }
    }

    #[test]
    fn duplicate_field_in_struct_literal_is_reported() {
        let d = struct_fields(TITIK);
        let (_, f) = lit_fields(
            "struct titik {\n    x int\n    y int\n}\nfn main() { p := titik{x: 1, x: 2} }",
        );
        match check_struct_lit_fields(&d, &f) {
            Some(FieldError::Duplicate { name }) => assert_eq!(name, "x"),
            _ => panic!("expected Duplicate"),
        }
    }

    #[test]
    fn exact_fields_are_silent() {
        let d = struct_fields(TITIK);
        let (_, f) = lit_fields(TITIK);
        assert!(check_struct_lit_fields(&d, &f).is_none());
    }

    #[test]
    fn partial_field_set_is_silent_go_allows_it() {
        let d = struct_fields(TITIK);
        let (_, f) = lit_fields(
            "struct titik {\n    x int\n    y int\n}\nfn main() { p := titik{x: 1} }",
        );
        assert!(check_struct_lit_fields(&d, &f).is_none());
    }

    #[test]
    fn dotted_and_elided_literals_are_skipped() {
        let (ty_dot, _) = lit_fields("fn main() { p := pkg.titik{x: 1} }");
        assert!(bare_struct_lit_type(&ty_dot).is_none(), "dotted type is foreign");
        let (ty_none, _) = lit_fields("fn main() { p := {x: 1} }");
        assert!(bare_struct_lit_type(&ty_none).is_none(), "elided type");
    }

    #[test]
    fn unknown_struct_is_never_reported() {
        let (_, f) = lit_fields("fn main() { p := titik{x: 1, z: 2} }");
        assert!(
            check_struct_lit_fields(&[], &f).is_none(),
            "an unregistered struct stays permissive"
        );
    }

    // ---------------- issue #152, slice 3: struct-literal field VALUES ---------

    fn lit_of(src: &str) -> Vec<FieldInit> {
        let (_, f) = lit_fields(src);
        f
    }

    #[test]
    fn string_literal_in_int_field_is_reported() {
        let d = struct_fields(TITIK);
        let f = lit_of("struct titik {\n    x int\n    y int\n}\nfn main() { p := titik{x: \"bukan\"} }");
        match check_field_value_literals(&d, &f) {
            Some(ValueError::Mismatch { field, declared, .. }) => {
                assert_eq!(field, "x");
                assert_eq!(declared, "int");
            }
            _ => panic!("expected Mismatch"),
        }
    }

    #[test]
    fn int_literal_in_string_field_is_reported() {
        let src = "struct s { nama string }\nfn main() { p := s{nama: 7} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(matches!(
            check_field_value_literals(&d, &f),
            Some(ValueError::Mismatch { .. })
        ));
    }

    #[test]
    fn bool_literal_in_int_field_is_reported() {
        let src = "struct s { n int }\nfn main() { p := s{n: true} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(matches!(
            check_field_value_literals(&d, &f),
            Some(ValueError::Mismatch { .. })
        ));
    }

    #[test]
    fn float_literal_in_int_field_is_reported() {
        let src = "struct s { n int }\nfn main() { p := s{n: 1.5} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(matches!(
            check_field_value_literals(&d, &f),
            Some(ValueError::Mismatch { .. })
        ));
    }

    #[test]
    fn int_literal_in_float_field_is_silent_go_converts_the_constant() {
        let src = "struct s { f float64 }\nfn main() { p := s{f: 2} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(check_field_value_literals(&d, &f).is_none());
    }

    #[test]
    fn matching_literals_are_silent() {
        let src = "struct s { a int  b string  c bool }\nfn main() { p := s{a: 1, b: \"x\", c: true} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(check_field_value_literals(&d, &f).is_none());
    }

    #[test]
    fn non_literal_values_are_never_reported() {
        // The type of an identifier is not knowable here (#152 will close that
        // separately); a false reject would be far worse than a missed one.
        let src = "struct s { n int }\nfn main() { v := 1\n p := s{n: v} }";
        let d = struct_fields(src);
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        let fnmain = e.fns().next().unwrap();
        let stmts = &fnmain.body.as_ref().unwrap().stmts;
        let mut f = Vec::new();
        for st in stmts {
            if let Stmt::Local(l) = st {
                if let Expr::StructLit { fields, .. } = &l.values[0] {
                    f = fields.clone();
                }
            }
        }
        assert!(check_field_value_literals(&d, &f).is_none());
    }

    #[test]
    fn user_defined_field_type_is_silent() {
        let src = "struct s { t titik }\nfn main() { p := s{t: 7} }";
        let d = struct_fields(src);
        let f = lit_of(src);
        assert!(check_field_value_literals(&d, &f).is_none());
    }
}
