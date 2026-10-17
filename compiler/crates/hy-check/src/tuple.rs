//! tuple.rs — T23: a multi-value result is not a tuple (HYB-E2002).
//!
//! SPEC-LANGUAGE.md §2.8 (T23) pins `p := minmax(3, 9)` as a compile-fail
//! claim (`// reject: HYB-E2002`), and diagnostics.json registers the code,
//! but nothing raised it: the checker accepted the program with
//! 0 diagnostics and `go build` then failed with `assignment mismatch:
//! 1 variable but minmax returns 2 values`. Accepted programs must emit
//! buildable Go, so the binding is rejected here, before emission.
//!
//! Scope (narrow on purpose — fire only on a mismatch we are certain of):
//! a define / `let` / `var` / plain `=` assignment whose single right-hand
//! side is a direct call to a file-local, non-shadowed top-level function
//! declaring two or more results, with a target count that differs from
//! the result count. `?`-unwrapped calls, method calls, package calls,
//! closures and foreign bindings stay permissive (their result counts are
//! not known here).

use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Expr, Stmt};

impl<'a> Checker<'a> {
    /// Pass-B entry, called once per flattened statement (nested block
    /// bodies are separate flat entries, so no recursion here).
    pub(crate) fn check_stmt_multi_value(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) if l.values.len() == 1 => {
                self.check_multi_value_bind(st, &l.values[0], l.names.len(), l.span);
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                span,
            } if values.len() == 1 => {
                self.check_multi_value_bind(st, &values[0], targets.len(), *span);
            }
            _ => {}
        }
    }

    fn check_multi_value_bind(&mut self, st: &FnState, value: &Expr, targets: usize, span: Span) {
        let Some((name, results)) = self.static_result_count(st, value) else {
            return;
        };
        if results < 2 || targets == results {
            return;
        }
        let msg = format!(
            "`{name}` returns {results} values, but {targets} {} bound: a multi-value result is not a tuple (T23)",
            if targets == 1 { "name is" } else { "names are" }
        );
        self.diags.push(
            Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                "why: Hybrid has no tuple type, and Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
            ]),
        );
    }

    /// `Some((callee, result count))` when `expr` is a direct call to a
    /// top-level function of this file that no local binding shadows.
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return None;
        };
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return None;
        };
        if self.lookup_binding(st, name).is_some() {
            return None; // shadowed by a local — not a static fn call
        }
        let decl = self.fns.get(name.as_str())?;
        Some((name.clone(), decl.results.len()))
    }
}
