//! tuple.rs — T23: a multi-value result is not a tuple (HYB-E2002).
//!
//! SPEC-LANGUAGE.md §2.8 (T23) pins `p := minmax(3, 9)` as a compile-fail
//! claim (`// reject: HYB-E2002`), and diagnostics.json registers the code,
//! but nothing raised it: the checker accepted the program with
//! 0 diagnostics and `go build` then failed with `assignment mismatch:
//! 1 variable but minmax returns 2 values`. Accepted programs must emit
//! buildable Go, so the binding is rejected here, before emission.
//!
//! Scope:
//! a define / `let` / `var` / plain `=` assignment whose single right-hand
//! side is a call declaring two or more results, with a target count that differs from
//! the result count. Covers direct top-level fn calls, fn-value bindings / aliases,
//! declared closures, and curated foreign package calls. `?`-unwrapped calls stay
//! permissive (unwrapping the error produces a single value).

use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Expr, Stmt, TypeExpr};

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

    /// `Some((callee, result count))` when `expr` is a direct or indirect call
    /// with statically known result count.
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return None;
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                // 1. Check local bindings (fn values, closures, aliases)
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(TypeExpr::Fn { results, .. }) = &b.ty {
                        return Some((name.clone(), results.len()));
                    }
                    let root = crate::freeze::resolve_alias(st, name);
                    if let Some(decl) = self.fns.get(root.as_str()) {
                        return Some((name.clone(), decl.results.len()));
                    }
                    if let Some(rb) = self.lookup_binding(st, &root) {
                        if let Some(TypeExpr::Fn { results, .. }) = &rb.ty {
                            return Some((name.clone(), results.len()));
                        }
                    }
                    return None;
                }
                // 2. Direct call to file-local top-level function
                if let Some(decl) = self.fns.get(name.as_str()) {
                    return Some((name.clone(), decl.results.len()));
                }
                None
            }
            Expr::FnLit { results, .. } => {
                Some(("fn".to_string(), results.len()))
            }
            Expr::Field { base, name: method_name, .. } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() {
                        if let Some(count) = foreign::package_result_count(pkg.as_str(), method_name.as_str()) {
                            return Some((format!("{pkg}.{method_name}"), count));
                        }
                    }
                }
                None
            }
            _ => {
                if let Some(TypeExpr::Fn { results, .. }) = self.ty_of_expr(st, callee) {
                    return Some(("fn".to_string(), results.len()));
                }
                None
            }
        }
    }
}
