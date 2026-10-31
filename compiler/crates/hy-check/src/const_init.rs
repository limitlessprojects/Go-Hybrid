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
use hy_syntax::{BinOp, Decl, Expr, LitKind, UnOp};

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
        if !st.is_none().then(|| self.file_const_decl(name)?.ty.as_ref()).flatten().is_some_and(|ty| matches!(self.resolve_type_alias(ty), hy_syntax::TypeExpr::Option { .. }) || !matches!(self.shape_of_type(ty), TypeShape::Primitive | TypeShape::Str)) && self.expr_is_constant_inner(value, st, &mut visiting, 0) {
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

    /// Compile-time constant expression under the narrow MVP rules that
    /// match what Go's `const` accepts for the shapes Hybrid emits.
    fn expr_is_constant_inner(
        &self,
        e: &Expr,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> bool {
        if ref_depth > 32 {
            return false;
        }
        match unwrap_parens(e) {
            Expr::Lit { kind, .. } => !matches!(kind, LitKind::Nil),
            Expr::Unary {
                op: UnOp::Neg | UnOp::Not,
                operand,
                ..
            } => self.expr_is_constant_inner(operand, st, visiting, ref_depth),
            Expr::Unary { op: UnOp::Addr, .. } => false,
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_is_constant_inner(lhs, st, visiting, ref_depth)
                    && self.expr_is_constant_inner(rhs, st, visiting, ref_depth)
            }
            Expr::Ident { name, .. } => {
                self.ident_is_constant(name, st, visiting, ref_depth + 1)
            }
            // Typed conversion of a constant (`int8(42)`, `byte(0xff)`) or
            // `len("const_str")`. A call to a real function (`get_val()`) or
            // shadowed local binding is never constant.
            Expr::Call { callee, args, .. } => {
                let Some(ty_name) = conversion_callee_name(callee) else {
                    return false;
                };
                if st.is_some_and(|s| s.env_lookup(ty_name).is_some()) {
                    return false;
                }
                if self.fns.contains_key(ty_name) || self.structs.contains_key(ty_name) || self.consts.contains_key(ty_name) {
                    return false;
                }
                // #1803: complex, real and imag are compile-time builtins when
                // their arguments are constant (Go spec, constant expressions).
                if ty_name == "complex" {
                    if self.type_aliases.contains_key("complex") {
                        return false;
                    }
                    return args.len() == 2
                        && !args[0].spread
                        && !args[1].spread
                        && self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth)
                        && self.expr_is_constant_inner(&args[1].value, st, visiting, ref_depth);
                }
                if ty_name == "real" || ty_name == "imag" {
                    if self.type_aliases.contains_key(ty_name) {
                        return false;
                    }
                    return args.len() == 1
                        && !args[0].spread
                        && self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth);
                }
                if args.len() != 1 || args[0].spread {
                    return false;
                }
                if ty_name == "len" {
                    if self.type_aliases.contains_key("len") {
                        return false;
                    }
                    return self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth)
                        && self.is_const_string_expr(&args[0].value, st, ref_depth);
                }
                if !self.is_const_conversion_target(ty_name) {
                    return false;
                }
                self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth)
            }
            // Everything else (field calls, index, receive, try, interp,
            // move, addr-of, composites nested under non-composite forms…)
            // is run-time.
            _ => false,
        }
    }

    fn is_const_conversion_target(&self, name: &str) -> bool {
        if let Some(ty) = self.type_aliases.get(name) {
            let resolved = self.resolve_type_alias(ty);
            if let hy_syntax::TypeExpr::Named { name: target, .. } = resolved {
                return !self.structs.contains_key(target.as_str())
                    && is_conversion_type_name(&target);
            }
            return false;
        }
        is_conversion_type_name(name)
    }

    fn is_const_string_conversion_target(&self, name: &str, st: Option<&FnState>) -> bool {
        if st.is_some_and(|s| s.env_lookup(name).is_some())
            || self.fns.contains_key(name)
            || self.structs.contains_key(name)
        {
            return false;
        }
        if let Some(ty) = self.type_aliases.get(name) {
            let resolved = self.resolve_type_alias(ty);
            return matches!(
                resolved,
                hy_syntax::TypeExpr::Named { name: target, .. }
                    if target == "string" && !self.structs.contains_key("string")
            );
        }
        name == "string"
    }

    fn is_const_string_expr(&self, e: &Expr, st: Option<&FnState>, ref_depth: usize) -> bool {
        if ref_depth > 32 {
            return false;
        }
        match unwrap_parens(e) {
            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            } => true,
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                self.is_const_string_expr(lhs, st, ref_depth)
                    && self.is_const_string_expr(rhs, st, ref_depth)
            }
            Expr::Ident { name, .. } => {
                if let Some(st) = st {
                    if let Some(b) = st.env_lookup(name) {
                        return b.immutable && matches!(b.shape, TypeShape::Str);
                    }
                }
                if let Some(cdecl) = self.file_const_decl(name) {
                    if let Some(ty) = &cdecl.ty {
                        if !matches!(self.shape_of_type(ty), TypeShape::Str) {
                            return false;
                        }
                    }
                    return self.is_const_string_expr(&cdecl.value, None, ref_depth + 1);
                }
                false
            }
            Expr::Call { callee, args, .. } => {
                let Some(ty_name) = conversion_callee_name(callee) else {
                    return false;
                };
                self.is_const_string_conversion_target(ty_name, st)
                    && args.len() == 1
                    && !args[0].spread
            }
            _ => false,
        }
    }

    fn file_const_decl(&self, name: &str) -> Option<&hy_syntax::ConstDecl> {
        self.file.decls.iter().find_map(|d| match d {
            Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })
    }

    fn ident_is_constant(
        &self,
        name: &str,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> bool {
        if ref_depth > 32 {
            return false;
        }
        // Local bindings and parameters shadow file-level constants.
        if let Some(st) = st {
            if let Some(b) = st.env_lookup(name) {
                // Only scalar/string `const` bindings are Go compile-time
                // constants; composite `const`s lower to Go `var`.
                return b.immutable && matches!(b.shape, TypeShape::Primitive | TypeShape::Str) && !matches!(b.ty.as_ref().map(|t| self.resolve_type_alias(t)), Some(hy_syntax::TypeExpr::Option { .. }));
            }
        }
        if visiting.iter().any(|v| v == name) {
            return false;
        }
        if let Some(cdecl) = self.file_const_decl(name) {
            if is_composite_lit(&cdecl.value) {
                return false;
            }
            if let Some(ty) = &cdecl.ty {
                if !matches!(self.shape_of_type(ty), TypeShape::Primitive | TypeShape::Str) {
                    return false;
                }
            }
            visiting.push(name.to_string());
            let ok = self.expr_is_constant_inner(&cdecl.value, None, visiting, ref_depth);
            visiting.pop();
            return ok;
        }
        false
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

/// Type names Hybrid spells that Go accepts as conversion targets in a
/// `const` initializer (`const x = int8(42)`).
pub(crate) fn is_conversion_type_name(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "string"
            | "byte"
            | "rune"
            | "char"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
            | "float32"
            | "float64"
            | "complex64"
            | "complex128"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "isize"
            | "usize"
    )
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
