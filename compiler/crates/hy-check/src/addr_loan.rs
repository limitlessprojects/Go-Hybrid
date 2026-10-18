//! addr_loan.rs — T24: no address-of on a loan (HYB-E3104).
//!
//! SPEC-LANGUAGE.md §2.9 (T24) pins a compile-fail claim:
//!
//! ```hy
//! n := &g.nodes[from]      // ok: map-slot loan (O8) — the only general-value `&`
//! p := &n                  // reject: HYB-E3104 — no address-of on a loan
//! ```
//!
//! Before this pass the second line was accepted with 0 diagnostics: the
//! emitter lowered the loan to a copy (`n := g.nodes[from]`) and `&n`
//! became a general Go pointer to that copy — the one value shape T24 says
//! Hybrid does not have, and one whose writes never reach the map slot.
//!
//! A binding is a slot loan when it was defined or re-assigned from
//! `&base[index]` (the same `addr_of_index_base_path` shape that creates
//! the A3 loan). The flag lives on the `Binding`, so a shadowing
//! redefinition in an inner block starts clean (no false rejects).

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt, Span};
use hy_syntax::{AssignOp, Expr, ForKind, InterpPart, LocalDecl, Place, PlaceKind, SelectArmKind, Stmt};

fn is_slot_loan_expr(e: &Expr) -> bool {
    if walk::addr_of_index_base_path(e).is_some() {
        return true;
    }
    let Some(inner) = as_addr_of(unwrap_parens(e)) else {
        return false;
    };
    matches!(unwrap_parens(inner), Expr::Index { .. })
}

impl<'a> Checker<'a> {
    /// After `process_local`: flag every name bound to `&m[k]` (including
    /// parenthesized `(&m[k])` and `&(m[k])`).
    pub(crate) fn mark_slot_loans_local(&self, st: &mut FnState, l: &LocalDecl) {
        if l.names.len() != l.values.len() {
            return;
        }
        for (name, val) in l.names.iter().zip(l.values.iter()) {
            if is_slot_loan_expr(val) {
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan = true;
                }
            }
        }
    }

    /// After `process_assign`: `x = &m[k]` makes `x` a slot loan; any other
    /// plain re-assignment of a whole binding clears the flag.
    pub(crate) fn mark_slot_loans_assign(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[hy_syntax::Expr],
    ) {
        if op != AssignOp::Assign || targets.len() != values.len() {
            return;
        }
        for (t, v) in targets.iter().zip(values.iter()) {
            if let PlaceKind::Ident(name) = &t.kind {
                let is_loan = is_slot_loan_expr(v);
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan = is_loan;
                }
            }
        }
    }

    /// T24: `&x` where `x` is a live slot-loan binding.
    pub(crate) fn check_addr_of_loan(&mut self, st: &FnState, uses: &[Use]) {
        for u in uses.iter().filter(|u| u.via_addr) {
            let Some(b) = self.lookup_binding(st, &u.name) else {
                continue;
            };
            if !b.slot_loan {
                continue;
            }
            self.emit_addr_of_loan_diag(&u.name, u.span);
        }
    }

    /// T24 extension: reject address-of on field/index projections rooted at a
    /// slot loan (`&n.val`, `&n.arr[0]`), and when `include_direct_ident` is
    /// true (inside closure bodies where `walk::own_uses` does not run), also
    /// reject direct `&n` / `&(n)` on a slot loan.
    pub(crate) fn check_stmt_addr_of_loan_projections(
        &mut self,
        st: &FnState,
        stmt: &Stmt,
        include_direct_ident: bool,
    ) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_addr_of_loan_projections(st, v, include_direct_ident);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.check_place_addr_of_loan_projections(st, t, include_direct_ident);
                }
                for v in values {
                    self.check_expr_addr_of_loan_projections(st, v, include_direct_ident);
                }
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr_addr_of_loan_projections(st, expr, include_direct_ident);
            }
            Stmt::Send { chan, value, .. } => {
                self.check_expr_addr_of_loan_projections(st, chan, include_direct_ident);
                self.check_expr_addr_of_loan_projections(st, value, include_direct_ident);
            }
            Stmt::If { cond, .. } => {
                self.check_expr_addr_of_loan_projections(st, cond, include_direct_ident);
            }
            Stmt::For { kind, .. } => match kind {
                ForKind::While { cond } => {
                    self.check_expr_addr_of_loan_projections(st, cond, include_direct_ident);
                }
                ForKind::Range { lo, hi, .. } => {
                    self.check_expr_addr_of_loan_projections(st, lo, include_direct_ident);
                    self.check_expr_addr_of_loan_projections(st, hi, include_direct_ident);
                }
                ForKind::Iterate { expr, .. } => {
                    self.check_expr_addr_of_loan_projections(st, expr, include_direct_ident);
                }
                ForKind::Infinite => {}
            },
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_addr_of_loan_projections(st, v, include_direct_ident);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_addr_of_loan_projections(st, callee, include_direct_ident);
                for a in args {
                    self.check_expr_addr_of_loan_projections(st, &a.value, include_direct_ident);
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_addr_of_loan_projections(st, subject, include_direct_ident);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        let mut arm_st = st.clone();
                        arm_st.scope_push();
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(b), ..
                        } = &arm.pattern
                        {
                            self.bind_match_arm(&mut arm_st, b, subject, false);
                        }
                        self.check_expr_addr_of_loan_projections(&arm_st, g, include_direct_ident);
                    }
                }
            }
            Stmt::Defer { call, .. } => {
                self.check_expr_addr_of_loan_projections(st, call, include_direct_ident);
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_addr_of_loan_projections(
                                st,
                                chan,
                                include_direct_ident,
                            );
                            self.check_expr_addr_of_loan_projections(
                                st,
                                value,
                                include_direct_ident,
                            );
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_addr_of_loan_projections(
                                st,
                                chan,
                                include_direct_ident,
                            );
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

    fn check_place_addr_of_loan_projections(
        &mut self,
        st: &FnState,
        place: &Place,
        include_direct_ident: bool,
    ) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_addr_of_loan_projections(st, base, include_direct_ident);
                self.check_expr_addr_of_loan_projections(st, index, include_direct_ident);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_addr_of_loan_projections(st, base, include_direct_ident);
            }
        }
    }

    pub(crate) fn check_expr_addr_of_loan_projections(
        &mut self,
        st: &FnState,
        expr: &Expr,
        include_direct_ident: bool,
    ) {
        if let Expr::AddrOf { operand, span }
        | Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            span,
        } = expr
        {
            match unwrap_parens(operand) {
                Expr::Ident { name, span: id_span } if include_direct_ident => {
                    if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag(name, *id_span);
                    }
                }
                Expr::Field { .. } => {
                    if let Some(root) = operand.root_ident() {
                        if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag(root, *span);
                        }
                    }
                }
                _ => {}
            }
        }
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_addr_of_loan_projections(st, lhs, include_direct_ident);
                self.check_expr_addr_of_loan_projections(st, rhs, include_direct_ident);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                self.check_expr_addr_of_loan_projections(st, operand, include_direct_ident);
            }
            Expr::Index { base, index, .. } => {
                self.check_expr_addr_of_loan_projections(st, base, include_direct_ident);
                self.check_expr_addr_of_loan_projections(st, index, include_direct_ident);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_addr_of_loan_projections(st, base, include_direct_ident);
                if let Some(lo) = lo {
                    self.check_expr_addr_of_loan_projections(st, lo, include_direct_ident);
                }
                if let Some(hi) = hi {
                    self.check_expr_addr_of_loan_projections(st, hi, include_direct_ident);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_addr_of_loan_projections(st, callee, include_direct_ident);
                for a in args {
                    self.check_expr_addr_of_loan_projections(st, &a.value, include_direct_ident);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_addr_of_loan_projections(st, &f.value, include_direct_ident);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_addr_of_loan_projections(st, i, include_direct_ident);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_addr_of_loan_projections(st, k, include_direct_ident);
                    self.check_expr_addr_of_loan_projections(st, v, include_direct_ident);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_addr_of_loan_projections(st, e, include_direct_ident);
                    }
                }
            }
            // Closure bodies are entered by emit_guard.rs with their own
            // scoped FnState and `include_direct_ident = true`.
            Expr::FnLit { .. } | Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn emit_addr_of_loan_diag(&mut self, name: &str, span: Span) {
        let msg = format!(
            "`&{}` takes the address of a loan: a map-slot loan is not an addressable value (T24)",
            name
        );
        let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), span, msg).with_notes(vec![
            format!(
                "`{}` is already the only general-value `&` Hybrid has (an O8 map-slot loan); a pointer to it would be a general pointer, which Tier 0 does not have",
                name
            ),
        ]);
        // The registry fix-its for E3104 describe the O2 return-escape
        // shape; replace them with the T24 one for this site.
        d.fixits = vec![FixIt {
            description: format!(
                "use `{}` directly, or take a fresh loan of the slot (`&m[k]`) where it is needed",
                name
            ),
            replacement: None,
        }];
        self.diags.push(d);
    }
}
