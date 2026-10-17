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

use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, UnOp};

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
        if self.expr_is_constant(value, st) {
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
    fn expr_is_constant(&self, e: &Expr, st: Option<&FnState>) -> bool {
        match unwrap_parens(e) {
            Expr::Lit { .. } => true,
            Expr::Unary {
                op: UnOp::Neg | UnOp::Not,
                operand,
                ..
            } => self.expr_is_constant(operand, st),
            Expr::Unary { op: UnOp::Addr, .. } => false,
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_is_constant(lhs, st) && self.expr_is_constant(rhs, st)
            }
            Expr::Ident { name, .. } => self.ident_is_constant(name, st),
            // Typed conversion of a constant: `int8(42)`, `byte(0xff)`.
            // A call to a real function (`get_val()`) is never constant.
            Expr::Call { callee, args, .. } => {
                let Some(ty_name) = conversion_callee_name(callee) else {
                    return false;
                };
                if !is_conversion_type_name(ty_name) {
                    return false;
                }
                // Known top-level fns win over type names if somehow shared.
                if self.fns.contains_key(ty_name) {
                    return false;
                }
                args.iter()
                    .all(|a| self.expr_is_constant(&a.value, st))
            }
            // Everything else (field calls, index, receive, try, interp,
            // move, addr-of, composites nested under non-composite forms…)
            // is run-time.
            _ => false,
        }
    }

    fn ident_is_constant(&self, name: &str, st: Option<&FnState>) -> bool {
        if self.consts.contains_key(name) {
            return true;
        }
        if let Some(st) = st {
            if let Some(b) = st.env_lookup(name) {
                // Only `const` bindings set `immutable` (let is shallow and
                // does not); matching Go: only const names are constant.
                return b.immutable;
            }
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
fn is_conversion_type_name(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "string"
            | "byte"
            | "rune"
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
