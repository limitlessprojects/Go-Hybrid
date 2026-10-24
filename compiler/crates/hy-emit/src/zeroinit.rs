//! zeroinit.rs — N5: a Hybrid-declared map's zero value auto-allocates.
//!
//! SPEC-LANGUAGE N5 (and T8, N3's zero-value table): `var m map[k]v;
//! m[k] = v` works — first write allocates — where Go panics on a nil map.
//! The emitter used to lower these declarations Go-exact, so an accepted
//! program built and then died with `assignment to entry in nil map`
//! (issue #645). D11 (keep N5 vs Go-exact) is still open; until it is
//! decided the emitted program follows the spec as written.
//!
//! Lowering: the zero value is allocated where it is DECLARED, never at the
//! write site — the write site would need the place's static type, and a
//! guard there evaluates the place twice. The spec makes the two the same
//! value: a Hybrid map has no `nil` (N1) and its zero value IS the empty
//! map (N3).
//!
//!   * `var m map[k]v`            → `var m map[k]v = map[k]v{}`
//!   * `var b bag` (bag has a map field, directly or through a nested
//!     struct field)              → `var b bag = bag{counts: map[string]int{}}`
//!   * `bag{}` / `bag{name: "x"}` → the unkeyed map fields are added
//!   * `new(bag)`                 → `&bag{counts: map[string]int{}}`
//!
//! Generic structs (`box[t]{}`) keep Go's zero value: their field types
//! need the literal's type arguments substituted, which the folded
//! `box[int]` name does not carry structurally. Arrays of such structs and
//! `make([]bag, n)` elements are likewise not covered by this lowering.

use std::collections::BTreeSet;

use hy_syntax::*;

use super::{sanitize_ident, Emitter};

/// Nested-struct depth guard. Go value types are acyclic (a struct cannot
/// contain itself by value), so this only bounds pathological inputs.
const MAX_DEPTH: usize = 16;

impl Emitter {
    /// The Go initializer that gives a zero value of `ty` allocated maps
    /// (N5), or None when Go's zero value already is the Hybrid one.
    pub(crate) fn n5_zero_init(&mut self, ty: &TypeExpr) -> Option<String> {
        self.n5_zero_init_at(ty, 0)
    }

    fn n5_zero_init_at(&mut self, ty: &TypeExpr, depth: usize) -> Option<String> {
        if depth > MAX_DEPTH {
            return None;
        }
        match self.n5_resolve_alias(ty) {
            TypeExpr::Map { .. } => Some(format!("{}{{}}", self.type_text(ty))),
            TypeExpr::Named { name, .. } => {
                let fields = self.n5_structs.get(&name)?.clone();
                let extra = self.n5_missing_fields_at(&fields, &BTreeSet::new(), depth + 1);
                if extra.is_empty() {
                    None
                } else {
                    Some(format!("{}{{{}}}", self.type_text(ty), extra.join(", ")))
                }
            }
            _ => None,
        }
    }

    /// `name: init` entries for the map-carrying fields of the struct
    /// literal type `ty` that the literal does not key itself (`given`).
    /// Empty for anything that is not a non-generic user struct.
    pub(crate) fn n5_missing_fields(&mut self, ty: &TypeExpr, given: &[FieldInit]) -> Vec<String> {
        let TypeExpr::Named { name, .. } = self.n5_resolve_alias(ty) else {
            return Vec::new();
        };
        let Some(fields) = self.n5_structs.get(&name).cloned() else {
            return Vec::new();
        };
        let given: BTreeSet<String> = given.iter().map(|f| f.name.clone()).collect();
        self.n5_missing_fields_at(&fields, &given, 1)
    }

    fn n5_missing_fields_at(
        &mut self,
        fields: &[FieldDecl],
        given: &BTreeSet<String>,
        depth: usize,
    ) -> Vec<String> {
        let mut out = Vec::new();
        for f in fields {
            if given.contains(&f.name) {
                continue;
            }
            if let Some(init) = self.n5_zero_init_at(&f.ty, depth) {
                out.push(format!("{}: {}", sanitize_ident(&f.name), init));
            }
        }
        out
    }

    /// `new(T)` where T's zero value needs N5 allocation → `&T{…}`. The
    /// parser encodes the type argument as a bare identifier (`new(bag)`)
    /// or as an empty type-as-value literal (`new(map[string]int)`).
    pub(crate) fn n5_new(&mut self, args: &[CallArg]) -> Option<String> {
        let [arg] = args else { return None };
        let ty = match &arg.value {
            Expr::Ident { name, span } => TypeExpr::Named {
                name: name.clone(),
                span: *span,
            },
            Expr::StructLit {
                ty: Some(t),
                fields,
                ..
            } if fields.is_empty() => t.clone(),
            Expr::MapLit {
                key,
                value,
                entries,
                span,
            } if entries.is_empty() => TypeExpr::Map {
                key: key.clone(),
                value: value.clone(),
                span: *span,
            },
            _ => return None,
        };
        if let TypeExpr::Named { name, .. } = &ty {
            // A local value named like a type is not a type argument.
            if self.local_vars.contains(name) {
                return None;
            }
        }
        self.n5_zero_init(&ty).map(|init| format!("&{}", init))
    }

    /// Follow user type aliases (`type counts = map[string]int`) to the
    /// type they name; anything else is returned as written.
    fn n5_resolve_alias(&self, ty: &TypeExpr) -> TypeExpr {
        let mut cur = ty.clone();
        for _ in 0..MAX_DEPTH {
            let next = match &cur {
                TypeExpr::Named { name, .. } => self.n5_aliases.get(name).cloned(),
                _ => None,
            };
            match next {
                Some(t) => cur = t,
                None => break,
            }
        }
        cur
    }
}
