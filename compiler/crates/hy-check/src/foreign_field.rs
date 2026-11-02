//! foreign_field.rs — a field SELECTION on a foreign handle that its binding
//! does not ship.
//!
//! Issue #2203 (Row 13 BUILDABLE). The method half of this hole is #2200,
//! fixed by `foreign::foreign_method_surface` at the method-call site. Field
//! selection was never checked, so `doc := serde_json.parse(...)?;
//! doc.totally_fake_field` passed `hyb check` with 0 diagnostics and died at
//! `go build` with `doc.TotallyFakeField undefined (type jsonbinding.Value has
//! no field or method TotallyFakeField)`.
//!
//! Only handles whose Go binding is vendored in `runtime/go/` (or is a
//! predeclared Go type) are modelled by `foreign::foreign_field_surface`;
//! every other receiver keeps the old fail-open behaviour, exactly as the
//! method-surface fix does.

use crate::foreign;
use crate::model::{unwrap_parens, Checker};
use crate::shape::receiver_type_name;
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Block, Expr, Stmt};

impl<'a> Checker<'a> {
    /// Pass-B entry for one statement. Nested block bodies are handled by the
    /// caller's flatten list, so `Stmt::Block` recurses explicitly here and
    /// nothing is walked twice.
    pub(crate) fn check_stmt_foreign_field(
        &mut self,
        st: &crate::model::FnState,
        stmt: &Stmt,
    ) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_foreign_field(st, v);
                }
            }
            Stmt::Assign { values, .. } => {
                // The write half needs the Place base resolved to an Expr;
                // the read half below covers the issue's repro and the
                // common shape. A write to an unknown field on a modelled
                // handle is tracked as a follow-up rather than guessed at
                // here.
                for v in values {
                    self.check_expr_foreign_field(st, v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_foreign_field(st, expr),
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_foreign_field(st, v);
                }
            }
            Stmt::If { cond, then, .. } => {
                self.check_expr_foreign_field(st, cond);
                self.check_block_foreign_field(st, then);
            }
            Stmt::For { body, .. } => self.check_block_foreign_field(st, body),
            Stmt::Block { block, .. } => self.check_block_foreign_field(st, block),
            _ => {}
        }
    }

    fn check_block_foreign_field(&mut self, st: &crate::model::FnState, b: &Block) {
        for s in &b.stmts {
            self.check_stmt_foreign_field(st, s);
        }
    }

    fn check_expr_foreign_field(&mut self, st: &crate::model::FnState, e: &Expr) {
        match e {
            Expr::Field { base, name, span } => {
                self.report_unknown_field(st, base, name, *span);
                self.check_expr_foreign_field(st, base);
            }
            Expr::Call { callee, args, .. } => {
                // A Field CALLEE is a method call — that half belongs to the
                // method surface (#2200), not here.
                if !matches!(unwrap_parens(callee), Expr::Field { .. }) {
                    self.check_expr_foreign_field(st, callee);
                }
                for a in args {
                    self.check_expr_foreign_field(st, &a.value);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_foreign_field(st, lhs);
                self.check_expr_foreign_field(st, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.check_expr_foreign_field(st, operand),
            Expr::Index { base, index, .. } => {
                self.check_expr_foreign_field(st, base);
                self.check_expr_foreign_field(st, index);
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let hy_syntax::InterpPart::Expr(x) = p {
                        self.check_expr_foreign_field(st, x);
                    }
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_foreign_field(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_foreign_field(st, i);
                }
            }
            _ => {}
        }
    }

    fn report_unknown_field(
        &mut self,
        st: &crate::model::FnState,
        base: &Expr,
        name: &str,
        span: Span,
    ) {
        let recv = self.shape_of_expr(st, base);
        let Some(tname) = receiver_type_name(&recv) else {
            return;
        };
        let Some(surface) = foreign::foreign_field_surface(&tname) else {
            return;
        };
        if surface.contains(&name) {
            return;
        }
        self.diags.push(Diagnostic::error(
            "HYB-E1002",
            self.path.clone(),
            span,
            format!(
                "`{}` has no field `{}` — Go refuses `{}.{} undefined (type {} has no field or method {})` (HYB-E1002)",
                tname, name, base.render(), name, tname, name
            ),
        ));
    }
}
