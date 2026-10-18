//! recv.rs — receive-operand channel-ness checks (HYB-E2008 / E3005).
//!
//! Issue #409: the send direction has been guarded since #77 —
//! `check_send_target` rejects a non-channel send target (HYB-E3007) and
//! fail-closes an unresolvable one (HYB-E3005). The receive direction had
//! no counterpart: `<-x` with `x` an int / map / non-channel call result
//! passed `hyb check` with 0 diagnostics and emitted Go that fails
//! `go build` (`cannot receive from non-channel`). This module mirrors the
//! send side one-for-one:
//!   * a receive whose operand resolves to a concrete non-channel shape is
//!     rejected (HYB-E2008);
//!   * an unresolvable operand stays fail-closed (HYB-E3005, O4.9) — the
//!     same treatment `check_send_target` gives an unknown send target.
//!
//! Wiring: every statement in the Tier-0 flat walk (lib.rs, after the
//! builtins pass) and every closure-body statement (closures.rs — FnLit
//! bodies sit outside the flat walk, the A5 perimeter) runs
//! `check_stmt_recv_operands`, which recurses the statement's own
//! expression trees (nested block statements arrive as their own flattened
//! entries) and classifies every `Expr::Receive` operand it finds, plus
//! `SelectArmKind::Recv` channel expressions.

use crate::model::{Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, InterpPart, PlaceKind, SelectArmKind, Stmt};

impl<'a> Checker<'a> {
    /// Statement entry: classify every receive operand in this statement's
    /// own expressions. Nested block bodies are NOT recursed — the Tier-0
    /// flat walk already lists those statements (same discipline as
    /// `arith.rs` / `builtins.rs`); the closure walk recurses explicitly.
    pub(crate) fn check_stmt_recv_operands(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    if let PlaceKind::Index { index, .. } = &t.kind {
                        self.check_expr_recv_operands(st, index);
                    }
                }
                for v in values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Split { base, mid, .. } => {
                // W-015: flat discipline — only the split statement's own
                // expressions; body statements are walked separately.
                self.check_expr_recv_operands(st, base);
                self.check_expr_recv_operands(st, mid);
            }
            Stmt::Expr { expr, .. } => self.check_expr_recv_operands(st, expr),
            Stmt::Send { chan, value, .. } => {
                // The send TARGET is `check_send_target`'s domain (E3007);
                // here we only look for receives NESTED in either side
                // (`ch <- <-done` and friends).
                self.check_expr_recv_operands(st, chan);
                self.check_expr_recv_operands(st, value);
            }
            // `if`/`for` init statements arrive as their own flattened
            // entries (walk.rs walk_one) — only the condition expression
            // belongs to this statement.
            Stmt::If { cond, .. } => self.check_expr_recv_operands(st, cond),
            Stmt::For { kind, .. } => match kind {
                hy_syntax::ForKind::While { cond } => self.check_expr_recv_operands(st, cond),
                hy_syntax::ForKind::Range { lo, hi, .. } => {
                    self.check_expr_recv_operands(st, lo);
                    self.check_expr_recv_operands(st, hi);
                }
                hy_syntax::ForKind::Iterate { expr, .. } => self.check_expr_recv_operands(st, expr),
                hy_syntax::ForKind::Infinite => {}
            },
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_recv_operands(st, callee);
                for a in args {
                    self.check_expr_recv_operands(st, &a.value);
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_recv_operands(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_recv_operands(st, g);
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_recv_operands(st, call),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_recv_operands(st, chan);
                            self.check_expr_recv_operands(st, value);
                        }
                        // The receive ARM's channel is a receive operand
                        // itself (`select { case v := <-x: }` with `x` an
                        // int used to pass with 0 diagnostics).
                        SelectArmKind::Recv { chan, .. } => {
                            let span = chan.span();
                            self.check_recv_operand(st, chan, span);
                            self.check_expr_recv_operands(st, chan);
                        }
                        SelectArmKind::Default => {}
                    }
                }
            }
            Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Block { .. }
            | Stmt::Managed { .. } => {}
        }
    }

    /// Recurse an expression tree, classifying every `Expr::Receive`
    /// operand found at any depth. FnLit bodies are the A5 perimeter —
    /// not walked here (the closure pass enters them).
    fn check_expr_recv_operands(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Receive { chan, span } => {
                self.check_recv_operand(st, chan, *span);
                self.check_expr_recv_operands(st, chan);
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_recv_operands(st, callee);
                for a in args {
                    self.check_expr_recv_operands(st, &a.value);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_recv_operands(st, lhs);
                self.check_expr_recv_operands(st, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.check_expr_recv_operands(st, operand),
            Expr::Index { base, index, .. } => {
                self.check_expr_recv_operands(st, base);
                self.check_expr_recv_operands(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_recv_operands(st, base);
                if let Some(lo) = lo {
                    self.check_expr_recv_operands(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_recv_operands(st, hi);
                }
            }
            Expr::Field { base, .. } => self.check_expr_recv_operands(st, base),
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_recv_operands(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_recv_operands(st, it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_recv_operands(st, k);
                    self.check_expr_recv_operands(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_recv_operands(st, e);
                    }
                }
            }
            Expr::FnLit { .. } | Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    /// Classify a receive's SOURCE — the mirror of `check_send_target`
    /// (issue #409): a receive is only well-typed when the channel
    /// expression resolves to a channel shape. A concrete non-channel is
    /// HYB-E2008; an unresolvable shape is fail-closed HYB-E3005 (O4.9),
    /// exactly as the send side treats an unknown target. Like the send
    /// side, an ill-typed source is not a sendability doubt — `managed`
    /// does not suppress it.
    pub(crate) fn check_recv_operand(&mut self, st: &FnState, chan: &Expr, span: Span) -> bool {
        match self.shape_of_expr(st, chan) {
            TypeShape::Chan(_) => true,
            TypeShape::Unknown => {
                let msg = format!(
                    "cannot prove `{}` is a channel — a receive source must resolve to `chan T` (O4.9)",
                    chan.render()
                );
                let notes = vec![
                    "why: the checker cannot classify the receive source's type, and a receive from a non-channel is not valid Go (issue #409)".to_string(),
                    "fix: bind the channel to a local (or parameter) with a visible `chan T` type before receiving, so the checker can resolve it".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3005", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
            other => {
                let msg = format!(
                    "cannot receive from `{}` ({}) — a receive source must be a channel (O4)",
                    chan.render(),
                    other.render()
                );
                let notes = vec![
                    "why: `<-` on a non-channel value is not valid Go; the accepted subset only emits buildable Go (issue #409 — the send side of this rule is HYB-E3007, issue #77)".to_string(),
                    "fix: receive from a `chan T` binding (`ch := make(chan T, n)` or a `chan T` parameter); a statement STARTING with `<-expr` is a receive — check the operand's type".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2008", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
        }
    }
}
