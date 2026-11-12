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

use crate::model::{
    as_addr_of, unwrap_parens, unwrap_parens_and_move, Checker, FnState, LoanTarget, TypeShape,
};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt, Span};
use hy_syntax::{
    AssignOp, Block, Expr, ForKind, InterpPart, LocalDecl, LocalKind, Place, PlaceKind,
    SelectArmKind, Stmt,
};

fn is_slot_loan_expr(ck: &Checker<'_>, st: &FnState, e: &Expr) -> bool {
    let _ = as_addr_of;
    walk::addr_of_index_base_path(e).is_some_and(|p| {
        ck.lookup_binding(st, p.split('.').next().unwrap_or(&p)).is_some_and(|b| {
            matches!(b.shape, TypeShape::Map(..) | TypeShape::Slice(_) | TypeShape::Array(_) | TypeShape::Struct(_) | TypeShape::Pointer(_))
        })
    })
}

fn is_slot_loan_or_alias_expr(
    ck: &Checker<'_>,
    st: &FnState,
    e: &Expr,
    target_shape: Option<&TypeShape>,
    allow_addr_root: bool,
) -> bool {
    if allow_addr_root && is_slot_loan_expr(ck, st, e) {
        return true;
    }
    let mut cur = e;
    while let Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. } = cur {
        cur = inner;
    }
    if matches!(cur, Expr::Ident { name, .. } if ck.lookup_binding(st, name).is_some_and(|b| b.slot_loan)) {
        return true;
    }
    let shape_ok = match target_shape {
        Some(sh) => ck.shape_carries_slot_loan_pointer(sh) || matches!(sh, TypeShape::Unknown),
        None => ck.expr_may_carry_slot_loan_pointer_shape(st, cur),
    };
    shape_ok && ck.expr_carries_slot_loan_pointer(st, cur)
}

impl<'a> Checker<'a> {
    /// Does `shape` carry a non-atomic pointer (or borrow handle) directly or
    /// inside a slice, array, map, or struct field? Scalar/value copies (`int`,
    /// `[]int`, `acct` without pointer fields) return `false` so O9/A4 reads
    /// from a slot loan are never over-rejected.
    pub(crate) fn shape_carries_slot_loan_pointer(&self, shape: &TypeShape) -> bool {
        self.shape_carries_slot_loan_pointer_depth(shape, 0)
    }

    fn shape_carries_slot_loan_pointer_depth(&self, shape: &TypeShape, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match shape {
            TypeShape::Pointer(inner) => !matches!(**inner, TypeShape::Atomic),
            TypeShape::Slice(elem) | TypeShape::Array(elem) => {
                self.shape_carries_slot_loan_pointer_depth(elem, depth + 1)
            }
            TypeShape::Map(k, v) => {
                self.shape_carries_slot_loan_pointer_depth(k, depth + 1)
                    || self.shape_carries_slot_loan_pointer_depth(v, depth + 1)
            }
            TypeShape::BorrowHandle(_) => true,
            TypeShape::Struct(Some(sname)) => self.structs.get(sname).is_some_and(|fields| {
                fields.iter().any(|(_, fty)| {
                    let fsh = self.shape_of_type(fty);
                    self.shape_carries_slot_loan_pointer_depth(&fsh, depth + 1)
                })
            }),
            _ => false,
        }
    }

    pub(crate) fn expr_may_carry_slot_loan_pointer_shape(&self, st: &FnState, e: &Expr) -> bool {
        let inner = unwrap_parens(e);
        if let Expr::Call { callee, .. } = inner {
            if let Some(results) = self.call_result_tys(st, callee) {
                return results.iter().any(|ty| {
                    let shape = self.shape_of_type(ty);
                    self.shape_carries_slot_loan_pointer(&shape)
                        || matches!(shape, TypeShape::Unknown)
                });
            }
        }
        let shape = self.shape_of_expr(st, inner);
        self.shape_carries_slot_loan_pointer(&shape) || matches!(shape, TypeShape::Unknown)
    }

    /// Collect active O8 slot-loan bindings whose loan pointer can be carried
    /// out of `e` (through composite literals, re-slices, projections, or
    /// pointer-returning calls such as `id_ptr(slot)` / `append(xs, slot)` /
    /// `some(slot)`). Stops at value-only projections and scalar-returning calls.
    pub(crate) fn collect_slot_loan_pointer_roots(
        &self,
        st: &FnState,
        e: &Expr,
        out: &mut Vec<String>,
    ) {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => {
                let is_active_ptr_loan = self.lookup_binding(st, name).is_some_and(|b| {
                    let is_loan = b.slot_loan
                        || st.loans.iter().any(|l| {
                            matches!(&l.target, LoanTarget::MapSlot { binding, .. } if binding == name)
                        });
                    is_loan
                        && (self.shape_carries_slot_loan_pointer(&b.shape)
                            || matches!(b.shape, TypeShape::Unknown))
                });
                if is_active_ptr_loan && !out.iter().any(|s| s == name) {
                    out.push(name.clone());
                }
            }
            Expr::Move { operand, .. } | Expr::Try { operand, .. } => {
                self.collect_slot_loan_pointer_roots(st, operand, out);
            }
            Expr::Slice { base, .. } => {
                if self.expr_may_carry_slot_loan_pointer_shape(st, e) {
                    self.collect_slot_loan_pointer_roots(st, base, out);
                }
            }
            Expr::Index { base, .. } | Expr::Field { base, .. } => {
                if self.expr_may_carry_slot_loan_pointer_shape(st, e) {
                    self.collect_slot_loan_pointer_roots(st, base, out);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.collect_slot_loan_pointer_roots(st, it, out);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.collect_slot_loan_pointer_roots(st, k, out);
                    self.collect_slot_loan_pointer_roots(st, v, out);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.collect_slot_loan_pointer_roots(st, &f.value, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                if self.expr_may_carry_slot_loan_pointer_shape(st, e) {
                    if let Expr::Field { base, .. } = unwrap_parens(callee) {
                        self.collect_slot_loan_pointer_roots(st, base, out);
                    }
                    for a in args {
                        self.collect_slot_loan_pointer_roots(st, &a.value, out);
                    }
                }
            }
            _ => {
                if is_slot_loan_expr(self, st, e) {
                    if let Some(root) = e.root_ident() {
                        if !out.iter().any(|s| s == root) {
                            out.push(root.to_string());
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn expr_carries_slot_loan_pointer(&self, st: &FnState, e: &Expr) -> bool {
        let mut roots = Vec::new();
        self.collect_slot_loan_pointer_roots(st, e, &mut roots);
        !roots.is_empty()
    }

    /// After `process_local`: flag every name bound to `&m[k]` (including
    /// parenthesized `(&m[k])` and `&(m[k])`), aliased from an active
    /// slot-loan binding (`p := slot`, `var p *T = slot`), or laundered into
    /// a pointer-carrying container / call result (`xs := []*T{slot}`,
    /// `p := id_ptr(slot)`, `p, _ := pair(slot)`).
    pub(crate) fn mark_slot_loans_local(&self, st: &mut FnState, l: &LocalDecl) {
        if !matches!(l.kind, LocalKind::Define | LocalKind::Let | LocalKind::Var) {
            return;
        }
        if l.names.len() == l.values.len() {
            let allow_addr_root =
                matches!(l.kind, LocalKind::Define | LocalKind::Let) && l.ty.is_none();
            let flags: Vec<bool> = l
                .names
                .iter()
                .zip(l.values.iter())
                .map(|(name, val)| {
                    let target_shape = self.lookup_binding(st, name).map(|b| b.shape.clone());
                    is_slot_loan_or_alias_expr(
                        self,
                        st,
                        val,
                        target_shape.as_ref(),
                        allow_addr_root,
                    )
                })
                .collect();
            for (name, is_loan) in l.names.iter().zip(flags) {
                if is_loan {
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = true;
                    }
                }
            }
        } else if l.values.len() == 1 && l.names.len() > 1 {
            if !self.expr_carries_slot_loan_pointer(st, &l.values[0]) {
                return;
            }
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            for (name, shape) in l.names.iter().zip(shapes.iter()) {
                if self.shape_carries_slot_loan_pointer(shape)
                    || matches!(shape, TypeShape::Unknown)
                {
                    if let Some(b) = st.env_lookup_mut(name) {
                        b.slot_loan = true;
                    }
                }
            }
        }
    }

    /// After `process_assign`: `x = &m[k]` or `x = slot` (plus pointer-carrying
    /// containers/calls and indexed container writes `xs[0] = slot`) makes the
    /// target carry `slot_loan`; a whole-value update (`x = T{...}`) to an
    /// already-active slot loan preserves `slot_loan` because the emitter keeps
    /// the O8 store-back active for `x` until its last use or shadowing decl.
    pub(crate) fn mark_slot_loans_assign(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[hy_syntax::Expr],
    ) {
        if op != AssignOp::Assign {
            return;
        }
        if targets.len() == values.len() {
            let allow_addr_root = targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
            let updates: Vec<(String, bool, bool)> = targets
                .iter()
                .zip(values.iter())
                .filter_map(|(t, v)| match &t.kind {
                    PlaceKind::Ident(name) => {
                        let target_shape = self.lookup_binding(st, name).map(|b| b.shape.clone());
                        let is_loan = is_slot_loan_or_alias_expr(
                            self,
                            st,
                            v,
                            target_shape.as_ref(),
                            allow_addr_root,
                        );
                        Some((name.clone(), is_loan, true))
                    }
                    PlaceKind::Index { .. } | PlaceKind::Field { .. } => {
                        let root = t.root().to_string();
                        if root.is_empty() {
                            return None;
                        }
                        let root_carries_ptr = self.lookup_binding(st, &root).is_some_and(|b| {
                            self.shape_carries_slot_loan_pointer(&b.shape)
                                || matches!(b.shape, TypeShape::Unknown)
                        });
                        let is_loan = root_carries_ptr && self.expr_carries_slot_loan_pointer(st, v);
                        Some((root, is_loan, false))
                    }
                    PlaceKind::AddrOf { .. } => None,
                })
                .collect();
            for (name, is_loan, is_whole_ident) in updates {
                if let Some(b) = st.env_lookup_mut(&name) {
                    if is_loan {
                        b.slot_loan = true;
                    } else if is_whole_ident
                        && matches!(
                            b.shape,
                            TypeShape::Slice(_) | TypeShape::Map(..) | TypeShape::Array(_)
                        )
                    {
                        b.slot_loan = false;
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            if !self.expr_carries_slot_loan_pointer(st, &values[0]) {
                return;
            }
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            let names: Vec<String> = targets
                .iter()
                .zip(shapes.iter())
                .filter_map(|(t, shape)| {
                    if !self.shape_carries_slot_loan_pointer(shape)
                        && !matches!(shape, TypeShape::Unknown)
                    {
                        return None;
                    }
                    let root = t.root();
                    (!root.is_empty()).then(|| root.to_string())
                })
                .collect();
            for name in names {
                if let Some(b) = st.env_lookup_mut(&name) {
                    b.slot_loan = true;
                }
            }
        }
    }

    /// T24 + O8: a live map-slot loan cannot be re-bound to a general
    /// pointer (`x = &T{}`, `x = p`, `x = nil`, multi-return `x, ok = f()`,
    /// or same-scope short redeclaration `x, tag := &T{}, 1` / `x, ok := f()`).
    /// The emitter lowers the loan to a copy of the slot plus a store-back at
    /// its last use, so the re-bind either emits a Go assignment of `*T` to a
    /// `T` variable that cannot build, or — for a pointer-typed binding — makes
    /// the store-back write the new pointee into the map slot. Runs BEFORE
    /// `process_local` / `mark_slot_loans_assign` so it sees the binding's prior form.
    pub(crate) fn check_slot_loan_rebind(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                ..
            } => {
                if targets.len() == values.len() {
                    let allow_addr_root =
                        targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
                    for (t, v) in targets.iter().zip(values.iter()) {
                        let PlaceKind::Ident(name) = &t.kind else {
                            continue;
                        };
                        let Some(b) = self.lookup_binding(st, name) else {
                            continue;
                        };
                        if !b.slot_loan
                            || matches!(
                                b.shape,
                                TypeShape::Slice(_) | TypeShape::Map(..) | TypeShape::Array(_)
                            )
                            || (allow_addr_root && walk::addr_of_index_base_path(v).is_some())
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
            Stmt::Local(l) if l.kind == LocalKind::Define && l.ty.is_none() => {
                if l.names.len() == l.values.len() {
                    for (name, v) in l.names.iter().zip(l.values.iter()) {
                        let Some(b) = Self::current_scope_binding(st, name) else {
                            continue;
                        };
                        if !b.slot_loan
                            || matches!(
                                b.shape,
                                TypeShape::Slice(_) | TypeShape::Map(..) | TypeShape::Array(_)
                            )
                            || walk::addr_of_index_base_path(v).is_some()
                            || !self.is_general_pointer_value(st, v)
                        {
                            continue;
                        }
                        self.emit_slot_loan_rebind_diag(name, &v.render(), l.span);
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
                    for (name, shape) in l.names.iter().zip(shapes.iter()) {
                        let Some(b) = Self::current_scope_binding(st, name) else {
                            continue;
                        };
                        if !b.slot_loan || !matches!(shape, TypeShape::Pointer(_)) {
                            continue;
                        }
                        self.emit_slot_loan_rebind_diag(name, &l.values[0].render(), l.span);
                    }
                }
            }
            _ => {}
        }
    }

    fn current_scope_binding<'s>(st: &'s FnState, name: &str) -> Option<&'s crate::model::Binding> {
        match st.scopes.last() {
            Some(top) => top.get(name),
            None => st.env.get(name),
        }
    }

    fn emit_slot_loan_rebind_diag(&mut self, name: &str, rhs_rendered: &str, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == hy_diag::Severity::Error && d.span == span)
        {
            return;
        }
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
        if let hy_syntax::Expr::Ident { name, .. } = unwrap_parens_and_move(v) {
            if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                return false;
            }
        }
        matches!(self.shape_of_expr(st, v), TypeShape::Pointer(_))
    }

    /// T24: `&x` or `move x` where `x` is a live slot-loan binding.
    pub(crate) fn check_addr_of_loan(&mut self, st: &FnState, uses: &[Use]) {
        for u in uses.iter().filter(|u| u.via_addr || u.via_move) {
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
                if let Some(ty) = &l.ty {
                    self.check_type_addr_of_loan_projections(st, ty, include_direct_ident);
                }
                let allow_slot_loan = matches!(l.kind, LocalKind::Define | LocalKind::Let)
                    && l.ty.is_none()
                    && l.names.len() == l.values.len();
                for v in &l.values {
                    // `x := &m[k]` is the O8 slot-loan initializer: legal when rooted at a place path.
                    let allow_root = allow_slot_loan && walk::addr_of_index_base_path(v).is_some();
                    self.walk_addr_of_loan_projections(st, v, include_direct_ident, allow_root);
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
                    // `x = &m[k]` is the slot-loan re-binding: legal when rooted at a place path.
                    let allow_root = allow_slot_loan && walk::addr_of_index_base_path(v).is_some();
                    self.walk_addr_of_loan_projections(st, v, include_direct_ident, allow_root);
                }
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr_addr_of_loan_projections(st, expr, include_direct_ident);
            }
            Stmt::Send { chan, value, is_move, span } => {
                if *is_move {
                    if let Some(root) = value.root_ident() {
                        self.check_addr_of_loan(
                            st,
                            &[Use {
                                name: root.into(),
                                span: *span,
                                is_write: false,
                                via_move: true,
                                via_addr: false,
                            }],
                        );
                    }
                }
                self.check_expr_addr_of_loan_projections(st, chan, include_direct_ident);
                self.check_expr_addr_of_loan_projections(st, value, include_direct_ident);
            }
            Stmt::If { cond, .. } => {
                self.check_expr_addr_of_loan_projections(st, cond, include_direct_ident);
            }
            Stmt::For { kind, body, .. } => {
                match kind {
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
                }
                let mut carry_st = st.clone();
                self.walk_for_loop_carried(&mut carry_st, kind, body, false);
                if let ForKind::While { cond } = kind {
                    self.check_expr_addr_of_loan_projections(&carry_st, cond, true);
                }
                if include_direct_ident {
                    self.walk_for_loop_carried(&mut carry_st, kind, body, true);
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    if let Expr::Ident { name, .. } = unwrap_parens_and_move(v) {
                        if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan)
                            && !st.borrow_bindings.contains(name)
                            && !st.loans.iter().any(|l| {
                                matches!(&l.target, LoanTarget::MapSlot { binding, .. } if binding == name)
                            })
                        {
                            self.emit_addr_of_loan_diag(name, v.span());
                        }
                    }
                    // LOAN-ESCAPE (O2, also HYB-E3104) already owns a
                    // returned `&m[k]`; recurse for nested borrows only.
                    let e = as_addr_of(unwrap_parens(v))
                        .filter(|_| self.map_slot_borrow_span(st, v).is_some())
                        .unwrap_or(v);
                    self.check_expr_addr_of_loan_projections(st, e, include_direct_ident);
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
                        SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                        } => {
                            if *is_move {
                                if let Some(root) = value.root_ident() {
                                    self.check_addr_of_loan(
                                        st,
                                        &[Use {
                                            name: root.into(),
                                            span: value.span(),
                                            is_write: false,
                                            via_move: true,
                                            via_addr: false,
                                        }],
                                    );
                                }
                            }
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
            PlaceKind::Field { base, .. } => {
                self.check_place_addr_of_loan_projections(st, base, include_direct_ident);
            }
            PlaceKind::AddrOf { base } => {
                if self.place_is_map_element_or_value_projection(st, base) {
                    self.emit_map_slot_borrow_diag(place.span);
                } else {
                    let root = base.root();
                    if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag(root, place.span);
                    }
                }
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
                Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
                    if let Some(root) = operand.root_ident() {
                        if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag(root, *span);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Expr::Move { operand, span } = expr {
            match unwrap_parens(operand) {
                Expr::Ident { name, .. } if include_direct_ident => {
                    if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag(name, *span);
                    }
                }
                Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
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
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.check_type_addr_of_loan_projections(st, t, include_direct_ident);
                }
                for f in fields {
                    self.check_expr_addr_of_loan_projections(st, &f.value, include_direct_ident);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.check_type_addr_of_loan_projections(st, t, include_direct_ident);
                }
                for i in items {
                    self.check_expr_addr_of_loan_projections(st, i, include_direct_ident);
                }
            }
            Expr::MapLit { key, value, entries, .. } => {
                self.check_type_addr_of_loan_projections(st, key, include_direct_ident);
                self.check_type_addr_of_loan_projections(st, value, include_direct_ident);
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
            Expr::FnLit { params, results, .. } => {
                for t in params.iter().map(|p| &p.ty).chain(results.iter().map(|r| &r.ty)) {
                    self.check_type_addr_of_loan_projections(st, t, include_direct_ident);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_type_addr_of_loan_projections(&mut self, st: &FnState, ty: &hy_syntax::TypeExpr, include_direct_ident: bool) {
        match ty {
            hy_syntax::TypeExpr::Array { len, elem, .. } => {
                if let Some(len_expr) = len {
                    self.check_expr_addr_of_loan_projections(st, len_expr, include_direct_ident);
                }
                self.check_type_addr_of_loan_projections(st, elem, include_direct_ident);
            }
            hy_syntax::TypeExpr::Pointer { to: inner, .. }
            | hy_syntax::TypeExpr::Slice { elem: inner, .. }
            | hy_syntax::TypeExpr::Chan { elem: inner, .. }
            | hy_syntax::TypeExpr::Option { elem: inner, .. } => {
                self.check_type_addr_of_loan_projections(st, inner, include_direct_ident);
            }
            hy_syntax::TypeExpr::Map { key, value, .. } => {
                self.check_type_addr_of_loan_projections(st, key, include_direct_ident);
                self.check_type_addr_of_loan_projections(st, value, include_direct_ident);
            }
            hy_syntax::TypeExpr::Fn { params, results, .. } => {
                for t in params.iter().map(|p| &p.ty).chain(results.iter()) {
                    self.check_type_addr_of_loan_projections(st, t, include_direct_ident);
                }
            }
            hy_syntax::TypeExpr::Named { .. } => {}
        }
    }

    fn emit_addr_of_loan_diag(&mut self, name: &str, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == hy_diag::Severity::Error && d.span == span)
        {
            return;
        }
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

    /// The `&m[k]` shape (including struct-field and fixed-array projections
    /// `&m[k].f`, `&m[k][i]`, `&m[k].arr[i]`) whose base resolves to a map:
    /// the address-of a map index expression or a value-projected sub-place,
    /// which Go refuses (`cannot take address of m[k]`). Only map bases and
    /// value-typed hops qualify — slice/pointer hops are addressable in Go.
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
        if self.is_map_element_or_value_projection(st, operand) {
            Some(span)
        } else {
            None
        }
    }

    fn is_map_element_or_value_projection(&self, st: &FnState, e: &Expr) -> bool {
        match unwrap_parens_and_move(e) {
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(..) => true,
                TypeShape::Array(_) => self.is_map_element_or_value_projection(st, base),
                _ => false,
            },
            Expr::Field { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Struct(_) => self.is_map_element_or_value_projection(st, base),
                _ => false,
            },
            _ => false,
        }
    }

    fn place_is_map_element_or_value_projection(&self, st: &FnState, p: &Place) -> bool {
        match &p.kind {
            PlaceKind::Index { base, .. } => match self.shape_of_place(st, base) {
                TypeShape::Map(..) => true,
                TypeShape::Array(_) => self.place_is_map_element_or_value_projection(st, base),
                _ => false,
            },
            PlaceKind::Field { base, .. } => match self.shape_of_place(st, base) {
                TypeShape::Struct(_) => self.place_is_map_element_or_value_projection(st, base),
                _ => false,
            },
            _ => false,
        }
    }

    fn shape_of_place(&self, st: &FnState, p: &Place) -> TypeShape {
        match &p.kind {
            PlaceKind::Ident(name) => self.lookup_binding(st, name).map(|b| b.shape.clone()).unwrap_or(TypeShape::Unknown),
            PlaceKind::Index { base, .. } => match self.shape_of_place(st, base) {
                TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => *v,
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => *v,
                    _ => TypeShape::Unknown,
                },
                _ => TypeShape::Unknown,
            },
            PlaceKind::Field { base, name } => self.field_shape(&self.shape_of_place(st, base), name),
            PlaceKind::AddrOf { base } => TypeShape::Pointer(Box::new(self.shape_of_place(st, base))),
        }
    }

    fn bind_shadow_locals_only(&self, st: &mut FnState, l: &LocalDecl) {
        if !matches!(l.kind, LocalKind::Define | LocalKind::Let | LocalKind::Var) {
            return;
        }
        let pre_st = st.clone();
        let shapes: Vec<TypeShape> = if l.names.len() == l.values.len() {
            l.values
                .iter()
                .map(|v| match (&l.ty, self.shape_of_expr(&pre_st, v)) {
                    (Some(t), TypeShape::Unknown) => self.shape_of_type(t),
                    (_, s) => s,
                })
                .collect()
        } else if l.values.len() == 1 && l.names.len() > 1 {
            self.multi_value_shapes(&pre_st, &l.values[0], l.names.len())
        } else {
            let sh = l
                .ty
                .as_ref()
                .map(|t| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown);
            vec![sh; l.names.len()]
        };
        for (name, shape) in l.names.iter().zip(shapes) {
            if Self::current_scope_binding(st, name).is_none() {
                self.define_binding(st, name, shape, l.ty.clone(), false, false, false);
            }
        }
    }

    fn walk_for_loop_carried(&mut self, st: &mut FnState, kind: &ForKind, body: &Block, emit: bool) {
        st.scope_push();
        match kind {
            ForKind::While { cond } if emit => self.check_expr_addr_of_loan_projections(st, cond, true),
            ForKind::Range { var, lo, hi, .. } => {
                if emit {
                    self.check_expr_addr_of_loan_projections(st, lo, true);
                    self.check_expr_addr_of_loan_projections(st, hi, true);
                }
                self.define_binding(st, var, TypeShape::Primitive, None, false, false, false);
            }
            ForKind::Iterate { vars, expr } => {
                if emit {
                    self.check_expr_addr_of_loan_projections(st, expr, true);
                }
                for v in vars {
                    self.define_binding(st, v, TypeShape::Unknown, None, false, false, false);
                }
            }
            _ => {}
        }
        self.walk_loop_carried_block(st, body, emit);
        st.scope_pop();
    }

    fn walk_loop_carried_block(&mut self, st: &mut FnState, b: &Block, emit: bool) {
        st.scope_push();
        for s in &b.stmts {
            self.walk_loop_carried_stmt(st, s, emit);
        }
        st.scope_pop();
    }

    fn walk_loop_carried_stmt(&mut self, st: &mut FnState, s: &Stmt, emit: bool) {
        if emit {
            self.check_slot_loan_rebind(st, s);
            if !matches!(s, Stmt::For { .. } | Stmt::If { .. }) {
                self.check_stmt_addr_of_loan_projections(st, s, true);
            }
        }
        match s {
            Stmt::Local(l) => {
                self.bind_shadow_locals_only(st, l);
                self.mark_slot_loans_local(st, l);
            }
            Stmt::Assign { targets, op, values, .. } => self.mark_slot_loans_assign(st, targets, *op, values),
            Stmt::If { init, cond, then, otherwise, .. } => {
                st.scope_push();
                if let Some(i) = init {
                    self.walk_loop_carried_stmt(st, i, emit);
                }
                if emit {
                    self.check_expr_addr_of_loan_projections(st, cond, true);
                }
                self.walk_loop_carried_block(st, then, emit);
                if let Some(other) = otherwise {
                    self.walk_loop_carried_stmt(st, other, emit);
                }
                st.scope_pop();
            }
            Stmt::For { kind, body, .. } => self.walk_for_loop_carried(st, kind, body, emit),
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => self.walk_loop_carried_block(st, body, emit),
            Stmt::Match { subject, arms, .. } => {
                for arm in arms {
                    st.scope_push();
                    if let hy_syntax::MatchPattern::Path { binding: Some(bname), .. } = &arm.pattern {
                        self.bind_match_arm(st, bname, subject, false);
                    }
                    self.walk_loop_carried_block(st, &arm.body, emit);
                    st.scope_pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    st.scope_push();
                    if let SelectArmKind::Recv { names, .. } = &arm.kind {
                        for n in names {
                            self.define_binding(st, n, TypeShape::Unknown, None, false, false, false);
                        }
                    }
                    self.walk_loop_carried_block(st, &arm.body, emit);
                    st.scope_pop();
                }
            }
            _ => {}
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

    #[test]
    fn closure_body_slot_loan_move_and_select_send_reject_e3104() {
        let spawn_codes = error_codes(
            r#"struct item {
    peer map[string]item
    n int
}
fn worker(p *item, done chan int) {
    p.peer["b"] = item{n: 2}
    done <- 1
}
fn main() {
    done := make(chan int, 1)
    f := fn() {
        m := map[string]item{}
        m["a"] = item{peer: m, n: 1}
        var slot *item
        slot = &m["a"]
        task worker(move slot, done)
        <-done
    }
    f()
}
"#,
        );
        assert!(
            spawn_codes.contains(&"HYB-E3104".to_string()),
            "got {:?}",
            spawn_codes
        );

        let select_codes = error_codes(
            r#"struct acct { bal int }
fn main() {
    ch := make(chan *acct, 1)
    f := fn() {
        m := map[string]acct{"a": acct{bal: 1}}
        s := &m["a"]
        select {
        ch <- move s:
            {}
        default:
            {}
        }
    }
    f()
}
"#,
        );
        assert!(
            select_codes.contains(&"HYB-E3104".to_string()),
            "got {:?}",
            select_codes
        );
    }

    #[test]
    fn slot_loan_alias_copy_and_loop_carried_move_reject_e3104() {
        let alias_codes = error_codes(
            r#"struct item {
    peer map[string]item
    n int
}
fn worker(p *item, done chan int) {
    p.peer["b"] = item{n: 2}
    done <- 1
}
fn main() {
    done := make(chan int, 1)
    m := map[string]item{}
    m["a"] = item{peer: m, n: 1}
    var slot *item
    slot = &m["a"]
    p := slot
    if false {
        slot.n = 2
    } else {
        task worker(move p, done)
    }
    <-done
}
"#,
        );
        assert!(
            alias_codes.contains(&"HYB-E3104".to_string()),
            "got {:?}",
            alias_codes
        );

        let loop_codes = error_codes(
            r#"struct acct { bal int }
fn worker(p *acct) {
    p.bal = 9
}
fn main() {
    m := map[string]acct{"a": acct{bal: 1}}
    var slot *acct
    for i in 0..2 {
        if i == 1 {
            task worker(move slot)
        }
        slot = &m["a"]
        slot.bal = i
    }
}
"#,
        );
        assert!(
            loop_codes.contains(&"HYB-E3104".to_string()),
            "got {:?}",
            loop_codes
        );
    }
}
