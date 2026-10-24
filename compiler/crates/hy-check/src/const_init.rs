//! const_init.rs — `const` initializers that are not constant expressions
//! (issue #205).
//!
//! `const c = get_val()` passes `hyb check` with 0 diagnostics today, then
//! fails `go build` with `get_val() (value of type int) is not constant`.
//! The emitter keeps the Go `const` keyword for every non-composite
//! initializer (`hy-emit` stmt.rs), so the accepted-should-emit-buildable-Go
//! contract requires a checker diagnostic (HYB-E6003).
//!
//! Composite literals (`[]T{…}`, maps, structs) already lower to Go `var`
//! and are intentionally out of scope here — their runtime-mutable /
//! sendability story is issue #188.

use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{BinOp, Decl, Expr, LitKind, TypeExpr, UnOp};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstTargetKind {
    Str,
    Bool,
    Numeric,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ConstValKind {
    Str,
    Bool,
    Numeric,
    /// Untyped local scalar `const` whose `Binding` carries `TypeShape::Primitive`
    /// without a resolved `TypeExpr` (numeric or boolean, never string).
    AnyPrimitive,
}

impl<'a> Checker<'a> {
    /// When `value` would be emitted as a Go `const` initializer but is not
    /// a constant expression, emit HYB-E6003.
    pub(crate) fn check_const_init_is_constant(
        &mut self,
        name: &str,
        value: &Expr,
        st: Option<&FnState>,
    ) {
        if is_composite_lit(value) {
            // Emitter rewrites these to `var` — not the #205 contract break.
            return;
        }
        let mut visiting = Vec::new();
        if st.is_none() {
            visiting.push(name.to_string());
        }
        if self
            .expr_const_kind(value, st, &mut visiting, 0)
            .is_some()
        {
            return;
        }
        let span = value.span();
        let msg = format!(
            "const `{}` initializer `{}` is not a constant expression (HYB-E6003)",
            name,
            value.render()
        );
        let notes = vec![
            "why: Go rejects a non-constant `const` initializer (`… is not constant`), so an accepted Hybrid program would emit unbuildable Go (issue #205)".to_string(),
            "fix: use a literal or constant expression (`const c = 42`, `const c = 1 + 2`), reference another `const`, or bind a run-time value with `:=` / `let` / `var`".to_string(),
            format!("note: declared here ({})", span_hint(span)),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E6003", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// Compile-time constant expression kind under the narrow MVP rules that
    /// match what Go's `const` accepts for the shapes Hybrid emits.
    fn expr_const_kind(
        &self,
        e: &Expr,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> Option<ConstValKind> {
        if ref_depth > 32 {
            return None;
        }
        match unwrap_parens(e) {
            Expr::Lit { kind, .. } => match kind {
                LitKind::Nil => None,
                LitKind::Str(_) | LitKind::RawStr(_) => Some(ConstValKind::Str),
                LitKind::Bool(_) => Some(ConstValKind::Bool),
                LitKind::Int(_) | LitKind::Float(_) | LitKind::Char(_) => {
                    Some(ConstValKind::Numeric)
                }
            },
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => {
                let k = self.expr_const_kind(operand, st, visiting, ref_depth)?;
                matches!(k, ConstValKind::Numeric | ConstValKind::AnyPrimitive)
                    .then_some(ConstValKind::Numeric)
            }
            Expr::Unary {
                op: UnOp::Not,
                operand,
                ..
            } => {
                let k = self.expr_const_kind(operand, st, visiting, ref_depth)?;
                matches!(k, ConstValKind::Bool | ConstValKind::AnyPrimitive)
                    .then_some(ConstValKind::Bool)
            }
            Expr::Unary { op: UnOp::Addr, .. } => None,
            Expr::Binary { op, lhs, rhs, .. } => {
                let lk = self.expr_const_kind(lhs, st, visiting, ref_depth)?;
                let rk = self.expr_const_kind(rhs, st, visiting, ref_depth)?;
                match op {
                    BinOp::Add => match (lk, rk) {
                        (ConstValKind::Str, ConstValKind::Str) => Some(ConstValKind::Str),
                        (
                            ConstValKind::Numeric | ConstValKind::AnyPrimitive,
                            ConstValKind::Numeric | ConstValKind::AnyPrimitive,
                        ) => Some(ConstValKind::Numeric),
                        _ => None,
                    },
                    BinOp::Sub
                    | BinOp::Mul
                    | BinOp::Div
                    | BinOp::Mod
                    | BinOp::BitAnd
                    | BinOp::BitOr
                    | BinOp::BitXor
                    | BinOp::Shl
                    | BinOp::Shr => match (lk, rk) {
                        (
                            ConstValKind::Numeric | ConstValKind::AnyPrimitive,
                            ConstValKind::Numeric | ConstValKind::AnyPrimitive,
                        ) => Some(ConstValKind::Numeric),
                        _ => None,
                    },
                    BinOp::And | BinOp::Or => match (lk, rk) {
                        (
                            ConstValKind::Bool | ConstValKind::AnyPrimitive,
                            ConstValKind::Bool | ConstValKind::AnyPrimitive,
                        ) => Some(ConstValKind::Bool),
                        _ => None,
                    },
                    BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                        Some(ConstValKind::Bool)
                    }
                }
            }
            Expr::Ident { name, .. } => self.ident_const_kind(name, st, visiting, ref_depth + 1),
            // Typed conversion of a compatible constant (`int8(42)`, `byte(0xff)`,
            // `string("abc")`, `bool(true)`) or `len("const_str")`. Integer-to-UTF-8
            // `string(65)` and cross-family conversions (`bool(1)`, `int("ok")`) are
            // rejected by Go in `const` initializers.
            Expr::Call { callee, args, .. } => {
                let ty_name = conversion_callee_name(callee)?;
                if st.is_some_and(|s| s.env_lookup(ty_name).is_some()) {
                    return None;
                }
                if self.fns.contains_key(ty_name) || self.structs.contains_key(ty_name) {
                    return None;
                }
                if args.len() != 1 || args[0].spread {
                    return None;
                }
                if ty_name == "len" {
                    if self.type_aliases.contains_key("len") {
                        return None;
                    }
                    let arg_kind =
                        self.expr_const_kind(&args[0].value, st, visiting, ref_depth)?;
                    return (arg_kind == ConstValKind::Str).then_some(ConstValKind::Numeric);
                }
                let target = self.const_conversion_target_kind(ty_name)?;
                let arg_kind = self.expr_const_kind(&args[0].value, st, visiting, ref_depth)?;
                match (target, arg_kind) {
                    (ConstTargetKind::Str, ConstValKind::Str) => Some(ConstValKind::Str),
                    (
                        ConstTargetKind::Bool,
                        ConstValKind::Bool | ConstValKind::AnyPrimitive,
                    ) => Some(ConstValKind::Bool),
                    (
                        ConstTargetKind::Numeric,
                        ConstValKind::Numeric | ConstValKind::AnyPrimitive,
                    ) => Some(ConstValKind::Numeric),
                    _ => None,
                }
            }
            // Everything else (field calls, index, receive, try, interp,
            // move, addr-of, composites nested under non-composite forms…)
            // is run-time.
            _ => None,
        }
    }

    fn const_conversion_target_kind(&self, name: &str) -> Option<ConstTargetKind> {
        if let Some(ty) = self.type_aliases.get(name) {
            return self.const_target_kind_of_type(ty);
        }
        conversion_target_kind_name(name)
    }

    fn const_target_kind_of_type(&self, ty: &TypeExpr) -> Option<ConstTargetKind> {
        let resolved = self.resolve_type_alias(ty);
        if let TypeExpr::Named { name: target, .. } = resolved {
            if self.structs.contains_key(target.as_str()) {
                return None;
            }
            return conversion_target_kind_name(&target);
        }
        None
    }

    fn file_const_decl(&self, name: &str) -> Option<&hy_syntax::ConstDecl> {
        self.file.decls.iter().find_map(|d| match d {
            Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })
    }

    fn ident_const_kind(
        &self,
        name: &str,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> Option<ConstValKind> {
        if ref_depth > 32 {
            return None;
        }
        // Local bindings and parameters shadow file-level constants.
        if let Some(st) = st {
            if let Some(b) = st.env_lookup(name) {
                if !b.immutable {
                    return None;
                }
                return match b.shape {
                    TypeShape::Str => Some(ConstValKind::Str),
                    TypeShape::Primitive => match b
                        .ty
                        .as_ref()
                        .and_then(|t| self.const_target_kind_of_type(t))
                    {
                        Some(ConstTargetKind::Str) => Some(ConstValKind::Str),
                        Some(ConstTargetKind::Bool) => Some(ConstValKind::Bool),
                        Some(ConstTargetKind::Numeric) => Some(ConstValKind::Numeric),
                        None => Some(ConstValKind::AnyPrimitive),
                    },
                    _ => None,
                };
            }
        }
        if visiting.iter().any(|v| v == name) {
            return None;
        }
        if let Some(cdecl) = self.file_const_decl(name) {
            if is_composite_lit(&cdecl.value) {
                return None;
            }
            let declared_kind = if let Some(ty) = &cdecl.ty {
                if !matches!(self.shape_of_type(ty), TypeShape::Primitive | TypeShape::Str) {
                    return None;
                }
                self.const_target_kind_of_type(ty)
            } else {
                None
            };
            visiting.push(name.to_string());
            let inner = self.expr_const_kind(&cdecl.value, None, visiting, ref_depth);
            visiting.pop();
            let inner = inner?;
            return Some(match declared_kind {
                Some(ConstTargetKind::Str) => ConstValKind::Str,
                Some(ConstTargetKind::Bool) => ConstValKind::Bool,
                Some(ConstTargetKind::Numeric) => ConstValKind::Numeric,
                None => inner,
            });
        }
        None
    }
}

fn is_composite_lit(e: &Expr) -> bool {
    matches!(
        unwrap_parens(e),
        Expr::SliceLit { .. } | Expr::MapLit { .. } | Expr::StructLit { .. }
    )
}

fn conversion_callee_name(callee: &Expr) -> Option<&str> {
    match unwrap_parens(callee) {
        Expr::Ident { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

fn conversion_target_kind_name(name: &str) -> Option<ConstTargetKind> {
    match name {
        "string" => Some(ConstTargetKind::Str),
        "bool" => Some(ConstTargetKind::Bool),
        "byte" | "rune" | "char" | "int" | "int8" | "int16" | "int32" | "int64" | "uint"
        | "uint8" | "uint16" | "uint32" | "uint64" | "uintptr" | "float32" | "float64" | "f32"
        | "f64" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64" | "isize"
        | "usize" => Some(ConstTargetKind::Numeric),
        _ => None,
    }
}

/// Type names Hybrid spells that Go accepts as conversion targets in a
/// `const` initializer (`const x = int8(42)`).
pub(crate) fn is_conversion_type_name(name: &str) -> bool {
    conversion_target_kind_name(name).is_some()
}

fn span_hint(span: Span) -> String {
    format!("{}:{}", span.start.line, span.start.col)
}

#[cfg(test)]
mod tests {
    use super::is_conversion_type_name;

    #[test]
    fn conversion_type_names_cover_go_builtins() {
        assert!(is_conversion_type_name("int8"));
        assert!(is_conversion_type_name("byte"));
        assert!(!is_conversion_type_name("get_val"));
        assert!(!is_conversion_type_name("fmt"));
    }
}
