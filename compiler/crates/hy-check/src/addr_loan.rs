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

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState, TypeShape};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt, Span};
use hy_syntax::{
    AssignOp, Expr, ForKind, InterpPart, LocalDecl, LocalKind, Place, PlaceKind, SelectArmKind,
    Stmt,
};

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
    fn value_is_slot_loan_or_alias(&self, st: &FnState, name: &str, val: &Expr) -> bool {
        if is_slot_loan_expr(val) {
            return true;
        }
        let Some(b) = self.lookup_binding(st, name) else {
            return false;
        };
        if !b.shape.is_reference_alias() {
            return false;
        }
        if let Some(src) = st.aliases.get(name) {
            let root = crate::freeze::resolve_alias(st, src);
            if self.lookup_binding(st, src).is_some_and(|sb| sb.slot_loan)
                || self.lookup_binding(st, &root).is_some_and(|rb| rb.slot_loan)
                || st.loans.iter().any(|l| {
                    matches!(
                        &l.target,
                        crate::model::LoanTarget::MapSlot { binding, .. }
                            if binding == src || binding == &root
                    )
                })
            {
                return true;
            }
        }
        if self.call_returns_fresh_copy(st, val) {
            return false;
        }
        let mut roots = Vec::new();
        crate::model::collect_ref_roots(val, &mut roots);
        crate::loans::collect_move_ident_sources(val, &mut roots);
        roots.iter().any(|r| {
            let root = crate::freeze::resolve_alias(st, r);
            self.lookup_binding(st, r).is_some_and(|sb| sb.slot_loan)
                || self.lookup_binding(st, &root).is_some_and(|rb| rb.slot_loan)
                || st.loans.iter().any(|l| {
                    matches!(
                        &l.target,
                        crate::model::LoanTarget::MapSlot { binding, .. }
                            if binding == r || binding == &root
                    )
                })
        })
    }

    /// After `process_local`: flag every name bound to `&m[k]` (including
    /// parenthesized `(&m[k])` and `&(m[k])`), a copy of a slot loan, or a
    /// return-forwarding call / multi-value unpack of one.
    pub(crate) fn mark_slot_loans_local(&self, st: &mut FnState, l: &LocalDecl) {
        let can_lower_direct =
            matches!(l.kind, LocalKind::Define | LocalKind::Let) && l.ty.is_none();
        if l.names.len() == l.values.len() {
            for (name, val) in l.names.iter().zip(l.values.iter()) {
                let is_loan = if is_slot_loan_expr(val) {
                    can_lower_direct
                } else {
                    self.value_is_slot_loan_or_alias(st, name, val)
                };
                if is_loan {
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = true;
                    }
                }
            }
        } else if l.values.len() == 1 && l.names.len() > 1 {
            for name in &l.names {
                if self.value_is_slot_loan_or_alias(st, name, &l.values[0]) {
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = true;
                    }
                }
            }
        }
    }

    /// After `process_assign`: `x = &m[k]` (or an alias/return-forwarding call
    /// of a slot loan) makes `x` a slot loan; a whole-value update (`x = T{...}`)
    /// to an already-active slot loan preserves `slot_loan`.
    pub(crate) fn mark_slot_loans_assign(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[hy_syntax::Expr],
    ) {
        if op != AssignOp::Assign
            || !targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)))
        {
            return;
        }
        if targets.len() == values.len() {
            for (t, v) in targets.iter().zip(values.iter()) {
                if let PlaceKind::Ident(name) = &t.kind {
                    let is_loan = self.value_is_slot_loan_or_alias(st, name, v);
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan |= is_loan;
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            for t in targets {
                if let PlaceKind::Ident(name) = &t.kind {
                    let is_loan = self.value_is_slot_loan_or_alias(st, name, &values[0]);
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan |= is_loan;
                    }
                }
            }
        }
    }

    /// T24 + O8: a live map-slot loan cannot be re-bound to a general
    /// pointer (`x = &T{}`, `x = p`, `x = nil`, or multi-return `x, ok = f()`).
    /// The emitter lowers the loan to a copy of the slot plus a store-back at
    /// its last use, so the re-bind either emits a Go assignment of `*T` to a
    /// `T` variable that cannot build, or — for a pointer-typed binding — makes
    /// the store-back write the new pointee into the map slot. Runs BEFORE
    /// `mark_slot_loans_assign` so it sees the binding's prior form.
    pub(crate) fn check_slot_loan_rebind(&mut self, st: &FnState, stmt: &Stmt) {
        let Stmt::Assign {
            targets,
            op: AssignOp::Assign,
            values,
            ..
        } = stmt
        else {
            return;
        };
        if targets.len() == values.len() {
            for (t, v) in targets.iter().zip(values.iter()) {
                let PlaceKind::Ident(name) = &t.kind else {
                    continue;
                };
                let Some(b) = self.lookup_binding(st, name) else {
                    continue;
                };
                if !b.slot_loan
                    || walk::addr_of_index_base_path(v).is_some()
                    || !self.is_general_pointer_value(st, v)
                {
                    continue;
                }
                self.emit_slot_loan_rebind_diag(name, &v.render(), t.span);
            }
        } else if values.len() == 1 && targets.len() > 1 {
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            for (t, shape) in targets.iter().zip(shapes.iter()) {
                let PlaceKind::Ident(name) = &t.kind else {
                    continue;
                };
                let Some(b) = self.lookup_binding(st, name) else {
                    continue;
                };
                if !b.slot_loan || !matches!(shape, TypeShape::Pointer(_)) {
                    continue;
                }
                self.emit_slot_loan_rebind_diag(name, &values[0].render(), t.span);
            }
        }
    }

    fn emit_slot_loan_rebind_diag(&mut self, name: &str, rhs_rendered: &str, span: Span) {
        let msg = format!(
            "`{} = {}` re-binds the map-slot loan `{}` to a general pointer: a loan is not a pointer (T24) and cannot hold one",
            name, rhs_rendered, name
        );
        let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), span, msg).with_notes(vec![
            "an O8 loan `&m[k]` is a checker fact, not a runtime pointer: it lowers to a copy of the slot plus a store-back at its last use, so the binding holds a value, not an address".to_string(),
            "re-binding it to a general pointer emits a Go assignment that cannot build, or makes the store-back write the new pointee into the map slot".to_string(),
        ]);
        d.fixits = vec![FixIt {
            description: format!(
                "bind the loan to its own name (`n := &m[k]`) and keep `{}` for the pointer",
                name
            ),
            replacement: None,
        }];
        self.diags.push(d);
    }

    /// A value that is a general Go pointer (or `nil`), not another loan.
    fn is_general_pointer_value(&self, st: &FnState, v: &hy_syntax::Expr) -> bool {
        if Checker::is_nil_literal(v) {
            return true;
        }
        if let hy_syntax::Expr::Ident { name, .. } = unwrap_parens(v) {
            if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                return false;
            }
        }
        matches!(self.shape_of_expr(st, v), TypeShape::Pointer(_))
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
                let allow_slot_loan = matches!(l.kind, LocalKind::Define | LocalKind::Let)
                    && l.ty.is_none()
                    && l.names.len() == l.values.len();
                for v in &l.values {
                    // `x := &m[k]` is the O8 slot-loan initializer: legal.
                    self.walk_addr_of_loan_projections(st, v, include_direct_ident, allow_slot_loan);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                ..
            } => {
                for t in targets {
                    self.check_place_addr_of_loan_projections(st, t, include_direct_ident);
                }
                let allow_slot_loan = *op == AssignOp::Assign
                    && targets.len() == values.len()
                    && targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
                for v in values {
                    // `x = &m[k]` is the slot-loan re-binding: legal.
                    self.walk_addr_of_loan_projections(st, v, include_direct_ident, allow_slot_loan);
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
                    // LOAN-ESCAPE (O2, also HYB-E3104) already owns a
                    // returned `&m[k]`; recurse for nested borrows only.
                    if self.map_slot_borrow_span(st, v).is_some() {
                        continue;
                    }
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

    /// Walk a whole expression for address-of-on-a-loan (T24). `Local` and
    /// `Assign` right-hand sides reach this through
    /// `walk_addr_of_loan_projections` with `allow_slot_loan_root = true`:
    /// those are the positions where the emitter lowers an `&m[k]` to the O8
    /// slot loan, so the root of the expression is exempt there.
    pub(crate) fn check_expr_addr_of_loan_projections(
        &mut self,
        st: &FnState,
        expr: &Expr,
        include_direct_ident: bool,
    ) {
        self.walk_addr_of_loan_projections(st, expr, include_direct_ident, false);
    }

    pub(crate) fn walk_addr_of_loan_projections(
        &mut self,
        st: &FnState,
        expr: &Expr,
        include_direct_ident: bool,
        allow_slot_loan_root: bool,
    ) {
        // Gap 2: `&m[k]` on a MAP is the O8 slot-loan form, and the emitter
        // lowers it only as the whole value of a binding. Anywhere else the
        // address-of is written straight into the Go, where a map index
        // expression is not addressable — the checker accepted those programs
        // and `go build` refused them ("cannot take address of
        // e.services[\"a\"]"). Slice and fixed-array elements (`&xs[0]`,
        // `&a[0]`) ARE addressable in Go and keep their lowering, so the
        // shape test is on the map, not on the index.
        if !allow_slot_loan_root {
            if let Some(span) = self.map_slot_borrow_span(st, expr) {
                self.emit_map_slot_borrow_diag(span);
            }
        }
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
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                self.check_expr_addr_of_loan_projections(st, operand, include_direct_ident);
            }
            // Parens are position-transparent (`is_slot_loan_expr` already
            // unwraps them, and the emitter lowers `(&m[k])` as a loan), so
            // `n := (&m[k])` is a binding position exactly like
            // `n := &m[k]` — keep the flag instead of clearing it.
            Expr::Paren { inner, .. } => {
                self.walk_addr_of_loan_projections(st, inner, include_direct_ident, allow_slot_loan_root);
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

    /// The `&m[k]` shape whose base resolves to a map: the address-of a map
    /// index expression, which Go refuses (`cannot take address of m[k]`).
    /// Only map bases qualify — slice/array elements are addressable.
    fn map_slot_borrow_span(&self, st: &FnState, e: &Expr) -> Option<Span> {
        let (operand, span) = match unwrap_parens(e) {
            Expr::AddrOf { operand, span } => (operand.as_ref(), *span),
            Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                span,
            } => (operand.as_ref(), *span),
            _ => return None,
        };
        match unwrap_parens(operand) {
            Expr::Index { base, .. } if matches!(self.shape_of_expr(st, base), TypeShape::Map(..)) => {
                Some(span)
            }
            _ => None,
        }
    }

    /// Gap 2 (T24 extension): `&m[k]` outside a binding value has no
    /// lowering — the emitter only turns a slot borrow into a copy +
    /// store-back when the borrow IS the whole value of a binding, so in an
    /// argument/operand position it emits `&m[k]` verbatim and `go build`
    /// refuses it (a map index expression is not addressable). One
    /// diagnostic per site: skip when another guard already reported this
    /// span (LOAN-ESCAPE O2 on `return &m[k]`, A6 borrow-in-field on
    /// `holder{p: &m[k]}`).
    fn emit_map_slot_borrow_diag(&mut self, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == hy_diag::Severity::Error && d.span == span)
        {
            return;
        }
        let mut d = Diagnostic::error(
            "HYB-E3104",
            self.path.clone(),
            span,
            "`&m[k]` here has no lowering: a map-slot borrow is only a loan where it is the whole value of a binding (`n := &m[k]`), and Go cannot take the address of a map element".to_string(),
        )
        .with_notes(vec![
            "the emitter lowers a slot loan to a copy of the slot plus a store-back at the binding's last use; in an argument or operand position there is no binding to store back to, so `&m[k]` is emitted as written".to_string(),
            "the emitted program does not build: `invalid operation: cannot take address of m[k] (map index expression)`".to_string(),
        ]);
        d.fixits = vec![FixIt {
            description: "bind the loan to its own name first (`n := &m[k]`) and pass `n`, or pass the element value and store it back after the call".to_string(),
            replacement: None,
        }];
        self.diags.push(d);
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_file, Verdict};
    use hy_diag::Severity;
    use hy_syntax::parse_file;

    fn error_codes(src: &str) -> Vec<String> {
        let ast = parse_file("inline.hy", src).expect("must parse");
        let res = check_file(&ast);
        assert_eq!(res.verdict, Verdict::Reject, "expected reject, got {:?}", res.diagnostics);
        res.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code.clone())
            .collect()
    }

    #[test]
    fn value_update_to_slot_loan_preserves_t24_addr_of_and_rebind_guards() {
        // After a whole-value update `q = acct{bal: 2}` on a live slot loan,
        // `q` still carries an active O8 store-back (`m["a"] = q`), so taking
        // `&q` or `&q.bal` must still reject with HYB-E3104.
        let codes = error_codes(
            r#"struct acct { bal int }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var q acct
    q = &m["a"]
    q = acct{bal: 2}
    p := &q
    fmt.println(p)
}
"#,
        );
        assert!(codes.contains(&"HYB-E3104".to_string()), "got {:?}", codes);

        let proj_codes = error_codes(
            r#"struct acct { bal int }
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var q acct
    q = &m["a"]
    q = acct{bal: 2}
    p := &q.bal
    fmt.println(p)
}
"#,
        );
        assert!(proj_codes.contains(&"HYB-E3104".to_string()), "got {:?}", proj_codes);
    }

    #[test]
    fn multi_return_pointer_rebind_of_slot_loan_rejects_e3104() {
        let codes = error_codes(
            r#"struct acct { bal int }
fn next_acct() (*acct, bool) {
    a := acct{bal: 5}
    return &a, true
}
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    q := &m["a"]
    var ok bool
    q, ok = next_acct()
    fmt.println(q.bal, ok)
}
"#,
        );
        assert!(codes.contains(&"HYB-E3104".to_string()), "got {:?}", codes);
    }

    #[test]
    fn closure_body_slot_loan_rebind_to_general_pointer_rejects_e3104() {
        let codes = error_codes(
            r#"struct acct { bal int }
fn main() {
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        q := &m["a"]
        q = &acct{bal: 5}
        fmt.println(q.bal)
    }
    f()
}
"#,
        );
        assert!(codes.contains(&"HYB-E3104".to_string()), "got {:?}", codes);
    }
}
