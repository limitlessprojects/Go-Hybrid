//! loans.rs — the loan rule family (SPEC §9.1 / §9.0 O2):
//!
//!   * A2  (O7)   HYB-E3101 — mutating-mode call on a boundary handle while
//!     a borrow-mode loan on the same handle is live.
//!   * A3  (O8)   HYB-E3102 — map insert/delete inside a live `&m[k]`
//!     map-slot loan window (creation → last use, non-lexical).
//!   * A6  (O12)  HYB-E3103 — struct fields never hold borrows (declared
//!     borrow-handle fields, and borrow values stored into fields).
//!   * LOAN-ESCAPE (O2) HYB-E3104 — a returned `&...` loan would outlive the
//!     callee frame; loans are function-local by construction.
//!   * A7  (O11/O13) HYB-E3003 rule A7 — consuming APIs transfer ownership;
//!     later uses are use-after-consume.
//!
//! Plus the environment updates that feed all of the above: local defines
//! (shapes, aliases, managed taint, A3 loan creation), assignments
//! (whole-binding rebinds, A3 insert detection, A6 field stores) and
//! returns (escape check, move marking).

use crate::foreign::{self, CallMode};
use crate::model::{
    as_addr_of, collect_ref_roots, move_operand_ident, unwrap_parens, Binding, Checker, FnState,
    Loan, LoanTarget, MoveKind, TypeShape,
};
use crate::shape::receiver_type_name;
use crate::walk::{self, CallRef};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Expr, LocalDecl, LocalKind, Place, PlaceKind};

// ---------------------------------------------------------------------------
// Call resolution + A2/A7 processing
// ---------------------------------------------------------------------------

enum Resolved {
    /// `pkg.method(...)` — the base ident is a known package, not a binding.
    Package {
        pkg: String,
        method: String,
    },
    /// `recv.method(...)` — receiver root binding + its shape.
    Method {
        root: String,
        method: String,
        recv: TypeShape,
    },
    /// `name(...)` — a plain function call (or builtin).
    Fn {
        name: String,
    },
    None,
}

impl<'a> Checker<'a> {
    fn resolve_call(&self, st: &FnState, cr: &CallRef) -> Resolved {
        match cr.callee {
            Expr::Field { base, name, .. } => {
                if let Expr::Ident { name: b, .. } = &**base {
                    if self.lookup_binding(st, b).is_none() && self.known_package(b) {
                        return Resolved::Package {
                            pkg: b.clone(),
                            method: name.clone(),
                        };
                    }
                }
                if let Some(root) = base.root_ident() {
                    let recv = self.shape_of_expr(st, base);
                    return Resolved::Method {
                        root: root.to_string(),
                        method: name.clone(),
                        recv,
                    };
                }
                Resolved::None
            }
            Expr::Ident { name, .. } => Resolved::Fn { name: name.clone() },
            _ => Resolved::None,
        }
    }

    /// Process one call site for the loan rules. `loop_end` is `Some(end)`
    /// when this call is a for-loop ITERATE expression: its borrow loans
    /// live across the whole loop body (O7's loop rule); otherwise borrow
    /// loans are call-scoped (A7 borrow-by-default, O11).
    pub(crate) fn process_call(
        &mut self,
        st: &mut FnState,
        cr: &CallRef,
        idx: usize,
        loop_end: Option<usize>,
    ) {
        match self.resolve_call(st, cr) {
            Resolved::Package { pkg, method } => {
                if foreign::package_mode(&pkg, &method) == CallMode::Borrow {
                    // a borrow-mode package call lends its handle-typed args
                    // for the call (`serde_json.to_string_pretty(doc)`)
                    self.lend_handle_args(st, cr, idx);
                }
            }
            Resolved::Method { root, method, recv } => {
                let Some(tname) = receiver_type_name(&recv) else {
                    return;
                };
                let receiver_moved = self
                    .lookup_binding(st, &root)
                    .map(|b| b.moved.is_some())
                    .unwrap_or(false);
                if receiver_moved {
                    // use-after-move/consume already reported at the use site
                    return;
                }
                match foreign::method_mode(&tname, &method) {
                    CallMode::Borrow => {
                        let last = loop_end.unwrap_or(idx);
                        st.loans.push(Loan {
                            target: LoanTarget::Handle {
                                handle: root.clone(),
                            },
                            created: idx,
                            last,
                            span: cr.span,
                            desc: format!(
                                "the borrow-mode call `{}` on `{}`{}",
                                cr.callee.render(),
                                root,
                                if loop_end.is_some() {
                                    " (live across the whole loop)"
                                } else {
                                    ""
                                }
                            ),
                        });
                    }
                    CallMode::Mutate => {
                        self.check_a2_conflict(st, cr, &root, idx);
                    }
                    CallMode::Consume => {
                        // A7: own-transfer INTO the API — receiver moved-from
                        self.mark_move(
                            st,
                            &root,
                            cr.span,
                            format!("the consuming call `{}`", cr.callee.render()),
                            MoveKind::Consumed,
                        );
                    }
                    _ => {}
                }
            }
            Resolved::Fn { name } => {
                if name == "delete" {
                    // A3: map delete is a map mutation
                    self.check_delete_conflict(st, cr, idx);
                }
                if self.fns.contains_key(&name) {
                    // A7 borrow-by-default: a handle passed to a Hybrid
                    // function is lent for the call only
                    self.lend_handle_args(st, cr, idx);
                }
            }
            Resolved::None => {}
        }
    }

    /// HYB-E3101 (A2): a mutating-mode call conflicts with any live
    /// borrow-mode loan on the same handle.
    fn check_a2_conflict(&mut self, st: &FnState, cr: &CallRef, root: &str, idx: usize) {
        let conflict = st.loans.iter().find(|l| {
            matches!(&l.target, LoanTarget::Handle { handle } if handle == root)
                && l.created <= idx
                && idx <= l.last
        });
        let Some(loan) = conflict else { return };
        let msg = format!("mutating call on `{}` while a borrow loan is live", root);
        let notes = vec![
            format!(
                "the borrow-mode loan on `{}` was created at {} ({}:{}) and is live until its last use — for a loop-header loan that is the whole loop body (O7, non-lexical window)",
                root,
                loan.desc,
                self.path,
                loan.span.start
            ),
            "a mutating call on a handle invalidates outstanding borrow-mode loans: the element views would alias mutated state".to_string(),
            "collect-then-mutate: reads at the boundary are copies — gather what you need into locals, let the loan die at its last use, THEN mutate (corpus 12 is the legal after-the-loop shape)".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3101", self.path.clone(), cr.span, msg).with_notes(notes),
        );
    }

    /// Handle-typed arguments of a call are lent for the call (call-scoped).
    fn lend_handle_args(&self, st: &mut FnState, cr: &CallRef, idx: usize) {
        for a in cr.args {
            if let Expr::Ident { name, .. } = &a.value {
                if let Some(b) = self.lookup_binding(st, name) {
                    if b.shape.is_handle() {
                        st.loans.push(Loan {
                            target: LoanTarget::Handle {
                                handle: name.clone(),
                            },
                            created: idx,
                            last: idx,
                            span: a.span,
                            desc: format!(
                                "the argument `{}` lent to `{}` (A7 borrow-by-default)",
                                name,
                                cr.callee.render()
                            ),
                        });
                    }
                }
            }
        }
    }

    /// A3 via `delete(m, k)`: a delete on a loaned base path inside the
    /// window invalidates the slot loan.
    fn check_delete_conflict(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        let Some(first) = cr.args.first() else { return };
        let Some(path) = walk::expr_path(&first.value) else {
            return;
        };
        self.emit_a3_if_live(st, &path, cr.span, idx, "delete");
    }

    /// Shared A3 emitter for index-assign inserts and `delete` calls.
    fn emit_a3_if_live(&mut self, st: &FnState, path: &str, span: Span, idx: usize, verb: &str) {
        let Some(loan) = st.loans.iter().find(|l| {
            matches!(&l.target, LoanTarget::MapSlot { path: p, .. } if p == path)
                && l.created <= idx
                && idx <= l.last
        }) else {
            return;
        };
        let (binding, loan_desc, loan_span, loan_last) = match &loan.target {
            LoanTarget::MapSlot { binding, .. } => {
                (binding.clone(), loan.desc.clone(), loan.span, loan.last)
            }
            _ => unreachable!("filtered above"),
        };
        let msg = format!("map {} invalidates the live slot loan `{}`", verb, binding);
        let notes = vec![
            format!(
                "{} ({}:{}) is a unique loan on a slot of `{}`, live from creation to its last use (O8, non-lexical window ending at statement {})",
                loan_desc,
                self.path,
                loan_span.start,
                path,
                loan_last
            ),
            "a rehash may move the slot — reorder so the slot mutation finishes before any insert/delete on the same map (corpus 09 is the legal twin), or copy-out / modify / store-back (reads are copies, O9)".to_string(),
        ];
        self.diags
            .push(Diagnostic::error("HYB-E3102", self.path.clone(), span, msg).with_notes(notes));
    }

    // -----------------------------------------------------------------------
    // Defines (environment + A3 loan creation)
    // -----------------------------------------------------------------------

    pub(crate) fn process_local(
        &mut self,
        st: &mut FnState,
        l: &LocalDecl,
        idx: usize,
        managed: bool,
    ) {
        // A3 loan creation: `svc := &e.services[existing]` (including parallel defines)
        if l.names.len() == l.values.len() {
            for (name, val) in l.names.iter().zip(l.values.iter()) {
                if let Some(path) = walk::addr_of_index_base_path(val) {
                    let binding = name.clone();
                    let last = st.last_use.get(&binding).copied().unwrap_or(idx).max(idx);
                    st.loans.push(Loan {
                        target: LoanTarget::MapSlot {
                            binding: binding.clone(),
                            path,
                        },
                        created: idx,
                        last,
                        span: val.span(),
                        desc: format!("`{} := {}`", binding, val.render()),
                    });
                }
            }
        }

        // bindings. T22 (SPEC-LANGUAGE.md): only `const` is DEEPLY
        // immutable and therefore class-4 sendable; `let` is SHALLOW
        // (interior mutation through an alias is still possible), so a
        // `let`-bound reference value must NOT receive the class-4
        // immutability shortcut in sendability.rs. Primitives, strings,
        // atomics and channels stay sendable via `is_sendable_by_type`.
        let immutable = matches!(l.kind, LocalKind::Const);
        if l.values.is_empty() {
            // `var hits atomic.int64` / `var got []int` — shape from the
            // ascription (there is no initializer to infer from).
            let shape =
                l.ty.as_ref()
                    .map(|t| self.shape_of_type(t))
                    .unwrap_or(TypeShape::Unknown);
            let ty = l.ty.clone();
            for name in &l.names {
                self.define_binding(st, name, shape.clone(), ty.clone(), immutable, managed);
            }
            return;
        }
        if l.values.len() == 1 && l.names.len() > 1 {
            // multi-value define from one expression
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            let tys = self.multi_value_tys(st, &l.values[0], l.names.len());
            for ((name, shape), ty) in l.names.iter().zip(shapes).zip(tys) {
                let (shape, ty) = match (&l.ty, shape) {
                    (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                    (_, s) => (s, ty.or_else(|| l.ty.clone())),
                };
                let is_view = matches!(shape, TypeShape::BorrowHandle(_));
                self.define_binding(st, name, shape, ty, immutable, managed);
                if is_view {
                    self.extend_call_loans_to_view(st, &l.values[0], name, idx);
                }
            }
        } else {
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                // N1: a bare `nil` initializer under a non-nilable
                // ascription is a compile error (HYB-E4001) — the SPEC's own
                // example is `var s []int = nil`.
                if let Some(t) = &l.ty {
                    if Checker::is_nil_literal(value) {
                        let shape = self.shape_of_type(t);
                        if !matches!(shape, TypeShape::Unknown)
                            && !Checker::is_nilable_shape(&shape)
                        {
                            self.emit_nil_type_error_text(
                                &shape,
                                t.render(),
                                value.span(),
                                &format!("the declared type of `{}`", name),
                            );
                        }
                    }
                }
                // N1 for composite literals: `job{m: nil}` carries its own
                // type context (the struct def / map value / slice elem) —
                // the nested nil needs no ascription to reject (HYB-E4001).
                self.check_composite_nils(st, value);
                // Ascription fills the gaps (O4.9 escape: "make the type
                // visible to the checker"): when the initializer's shape
                // does not resolve, the declared type decides. When inference
                // DOES resolve it keeps priority — it is more precise than
                // the ascription for boundary handles (`var v serde_json.view
                // = doc.array()` must stay a BorrowHandle for the loan
                // windows); a lying ascription is caught downstream by the
                // emitted Go build (the emitter carries the declared type).
                let inferred = self.shape_of_expr(st, value);
                let (shape, ty) = match (&l.ty, inferred) {
                    (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                    (_, inferred) => (inferred, self.ty_of_expr(st, value)),
                };
                // `x := move y` — y is moved; x takes ownership (no alias);
                // non-ident moves (`x := move s.data`) trigger HYB-E3004 (O5.4).
                let moved_non_owned = move_operand_ident(value)
                    .and_then(|src| self.lookup_binding(st, &src))
                    .and_then(|b| b.non_owned_origin.clone());
                self.mark_move_in_value(
                    st,
                    value,
                    format!("the move-define `{}`", value.render()),
                );
                if move_operand_ident(value).is_some() {
                    self.define_binding(st, name, shape, ty, immutable, managed);
                    if let Some(origin) = moved_non_owned {
                        if let Some(b) = st.env_lookup_mut(name) {
                            b.non_owned_origin = Some(origin);
                        }
                    }
                    continue;
                }
                let is_view = matches!(shape, TypeShape::BorrowHandle(_));
                self.define_binding(st, name, shape, ty, immutable, managed);
                // A2 non-lexical extension: `u := doc.array(...)` stores an
                // element VIEW — the borrow loan the call opened on the
                // handle lives until the view's last use (O7), not just the
                // call. Only borrow-handle results extend the loan: boundary
                // reads of primitives are copies (O9) — a copied-out int
                // must NOT keep the handle borrowed.
                if is_view {
                    self.extend_call_loans_to_view(st, value, name, idx);
                }
                // may-share recording (#27): a shallow struct/array copy, a
                // re-slice, a field/index projection, or a composite literal
                // that embeds a live reference binding leaves `name` sharing
                // mutable GC storage with its source — so a later `move` of
                // either must freeze the other (O5.2).
                self.record_may_share(st, name, value);
                // alias recording: `m2 := agg` (single-parent link, the
                // friendlier "frozen alias" diagnostic) — and call results
                // that hand back argument storage (#38).
                self.record_alias(st, name, value);
            }
        }
    }


    /// Does the local binding `name` denote a value that a shallow Go copy
    /// would share mutable GC storage with? Locals/params only (`const`
    /// values are file-level immutables, not freeze participants).
    fn binding_may_share(&self, st: &FnState, name: &str) -> bool {
        st.env_lookup(name)
            .map(|b| self.shape_may_share(&b.shape))
            .unwrap_or(false)
    }

    /// Record may-share edges for a define/rebind `name <op> value` (#27).
    /// Covers the four reported bypass forms: `h2 := h1` (shallow struct/
    /// array copy), `m2 := (agg)` / `s2 := s1[:]` (parenthesized / re-sliced
    /// RHS), `m2 := h1.m` (field/index projection), and `h := holder{m: agg}`
    /// (composite literal capturing a live binding) — plus the CALL forms of
    /// #73: a shallow copy or generic identity that returns argument
    /// storage through `f(...)` (no interprocedural summary, same argument
    /// as #38's alias recording).
    fn record_may_share(&self, st: &mut FnState, name: &str, value: &Expr) {
        match unwrap_parens(value) {
            Expr::Ident { name: src, .. } => {
                if self.binding_may_share(st, src) {
                    st.share_edge(name, src);
                }
            }
            // `s2 := s1[:]` — the result shares s1's backing array
            // `m2 := h1.m` / `m2 := arr[i]` — projecting a reference-bearing
            // value out leaves it aliasing the composite it was read from
            // composite literals that embed live reference bindings
            // a call that hands back storage reachable from an argument (#73)
            //
            // All of these — plus the WRAPPED forms reported as the "#73
            // tail": nested calls (`t := same(same(m))`), views of call
            // results (`t := same(m)[0:2]`), literals holding call results
            // (`w := wrap{items: same(m)}`), fields read out of a literal
            // (`t := box{items: m}.items`) and parameter-field returns
            // (`t := take(box{items: m})`) — are resolved by walking the whole
            // value with `collect_ref_roots` instead of `Expr::root_ident()`,
            // which stops at a call or a literal base. The edge is still only
            // recorded for roots that may share mutable GC storage, so scalar
            // copy-outs (`n := len(m)`) stay unedged.
            _ => {
                let inner = unwrap_parens(value);
                let result = self.shape_of_expr(st, inner);
                let is_literal = matches!(
                    inner,
                    Expr::StructLit { .. } | Expr::MapLit { .. } | Expr::SliceLit { .. }
                );
                let carries = self.shape_may_share(&result)
                    || matches!(result, TypeShape::Unknown)
                    || inner.root_ident().is_some()
                    || is_literal;
                if !carries {
                    return;
                }
                let mut roots = Vec::new();
                collect_ref_roots(value, &mut roots);
                for root in roots {
                    if root == name {
                        continue;
                    }
                    if self.binding_may_share(st, &root) {
                        st.share_edge(name, &root);
                    }
                }
            }
        }
    }

    // `record_literal_ref` (pr-28's composite-literal recursion) is gone:
    // `record_may_share` now walks literals, calls and views with
    // `collect_ref_roots`, so a literal field that embeds a live binding —
    // bare (`holder{m: agg}`), projected (`holder{m: h1.m}`) or produced by
    // a call (`wrap{items: same(agg)}`) — is covered by one collector.

    /// Record a binding-level alias created by `name := value` (#38).
    ///
    /// The original check only saw a bare reference-typed `Ident` copy
    /// (`m2 := agg`). But a CALL that returns a reference-typed value can
    /// hand back storage owned by one of its arguments — `same(m)`,
    /// `append(xs, v)`, and every filter/trim/identity helper do it — and
    /// the checker has no interprocedural summary to rule that out. Record
    /// the result as an alias of the first reference-typed argument binding,
    /// so a later `move` of either freezes the other (O5.2). `move`-wrapped
    /// arguments transfer ownership into the callee and are skipped. The
    /// new binding also inherits the source's non_owned_origin (O5.1): an
    /// alias of a parameter/receiver/range variable is itself non-owned.
    fn record_alias(&self, st: &mut FnState, name: &str, value: &Expr) {
        match unwrap_parens(value) {
            Expr::Ident { name: src, .. } => {
                if let Some(b) = self.lookup_binding(st, src) {
                    let src_origin = b.non_owned_origin.clone();
                    if b.shape.is_reference_alias() {
                        st.aliases.insert(name.to_string(), src.clone());
                        if let Some(origin) = src_origin {
                            if let Some(nb) = st.env_lookup_mut(name) {
                                nb.non_owned_origin = Some(origin);
                            }
                        }
                    }
                }
            }
            // A reference-shaped result that is not a bare binding: the call
            // forms of #38 (`t := same(m)`) and the wrapper tail — nested
            // calls (`t := same(same(m))`), views of call results
            // (`t := same(m)[0:2]`), literals holding call results
            // (`w := wrap{items: same(m)}`), fields read out of a literal
            // (`t := box{items: m}.items`) and parameter-field returns
            // (`t := take(box{items: m})`). `collect_ref_roots` walks the
            // whole value, so every root binding the result could alias is
            // considered; the first reference-typed one wins (the freeze is a
            // single-parent link) and `move`-wrapped operands, which transfer
            // ownership into the callee, are skipped by the collector.
            other
                if self
                    .shape_of_expr(st, unwrap_parens(other))
                    .is_reference_alias() =>
            {
                let mut roots = Vec::new();
                collect_ref_roots(value, &mut roots);
                for src in roots {
                    if src == name {
                        continue;
                    }
                    let matched = self
                        .lookup_binding(st, &src)
                        .map(|b| (b.shape.is_reference_alias(), b.non_owned_origin.clone()));
                    if let Some((true, origin)) = matched {
                        st.aliases.insert(name.to_string(), src.clone());
                        if let Some(origin) = origin {
                            if let Some(nb) = st.env_lookup_mut(name) {
                                nb.non_owned_origin = Some(origin);
                            }
                        }
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    /// Record O5.2 alias links for pointer borrows (`p := &stats`, `slot := &m[k]`)
    /// so moving `p` freezes the underlying mutable binding `stats` (#33).
    pub(crate) fn record_local_borrow_aliases(&self, st: &mut FnState, l: &LocalDecl) {
        if l.names.len() == l.values.len() {
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                if move_operand_ident(value).is_none() {
                    if let Some(op) = as_addr_of(unwrap_parens(value)) {
                        if let Some(src) = op.root_ident() {
                            if src != name {
                                if let Some(b) = self.lookup_binding(st, src) {
                                    if !b.shape.is_sendable_by_type() {
                                        st.aliases.insert(name.clone(), src.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Record O5.2 alias links for pointer-borrow rebinds (`p = &stats`).
    pub(crate) fn record_assign_borrow_aliases(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) {
        if op == AssignOp::Assign && values.len() == targets.len() {
            for (t, v) in targets.iter().zip(values.iter()) {
                if let PlaceKind::Ident(name) = &t.kind {
                    if move_operand_ident(v).is_none() {
                        if let Some(addr_op) = as_addr_of(unwrap_parens(v)) {
                            if let Some(src) = addr_op.root_ident() {
                                if src != name {
                                    if let Some(b) = self.lookup_binding(st, src) {
                                        if !b.shape.is_sendable_by_type() {
                                            st.aliases.insert(name.clone(), src.to_string());
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Extend call-scoped borrow loans opened by the calls inside `value`
    /// to the last use of the view binding `name` (the A2 non-lexical
    /// window for define-stored element views).
    fn extend_call_loans_to_view(&self, st: &mut FnState, value: &Expr, name: &str, idx: usize) {
        let mut calls = Vec::new();
        walk::expr_calls(value, &mut calls);
        let mut roots = Vec::new();
        for cr in &calls {
            if let Resolved::Method { root, method, recv } = self.resolve_call(st, cr) {
                if let Some(t) = receiver_type_name(&recv) {
                    if foreign::method_mode(&t, &method) == CallMode::Borrow {
                        roots.push(root);
                    }
                }
            }
        }
        if roots.is_empty() {
            return;
        }
        let view_last = st.last_use.get(name).copied().unwrap_or(idx).max(idx);
        for loan in st.loans.iter_mut() {
            if let LoanTarget::Handle { handle } = &loan.target {
                if loan.created == idx && loan.last == idx && roots.iter().any(|r| r == handle) {
                    loan.last = view_last;
                }
            }
        }
    }

    pub(crate) fn define_binding(
        &self,
        st: &mut FnState,
        name: &str,
        shape: TypeShape,
        ty: Option<hy_syntax::TypeExpr>,
        immutable: bool,
        managed: bool,
    ) {
        st.aliases.remove(name);
        st.share_clear(name);
        st.env_define(
            name,
            Binding {
                shape,
                ty,
                immutable,
                managed_taint: managed, // O6.2: born inside `managed {}`
                moved: None,
                non_owned_origin: None,
            },
        );
    }

    // -----------------------------------------------------------------------
    // Assignments (A3 inserts, A6 field stores, whole-binding rebinds)
    // -----------------------------------------------------------------------

    pub(crate) fn process_assign(
        &mut self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
        _span: Span,
        idx: usize,
    ) {
        // T22 (issue #37): `const` is deeply immutable — a rebind, an
        // element/index write, or a field store whose ROOT resolves to a
        // `const` binding is HYB-E3006. `Binding::immutable` is set iff
        // the binding was declared `const` (fn-local or file-level), and
        // lookup is locals-first, so a shadowing `let`/`var` copy stays
        // writable. Rejected targets are excluded from the A3/A6/N1 and
        // rebind bookkeeping below: the verdict is already reject.
        let is_const_write: Vec<bool> = targets
            .iter()
            .map(|t| match self.const_write_root(st, t) {
                Some(name) => {
                    let msg = format!(
                        "cannot write to `{}` — it is a `const` (deeply immutable)",
                        name
                    );
                    let notes = vec![
                        "`const` is a compile-time constant: no rebind, no element write, no field store through it (SPEC-LANGUAGE T22)".to_string(),
                        "a file-level `const` lowers to package-level state every task body reaches ambiently — an accepted write would be a data race with zero spawn-site evidence (issue #37)".to_string(),
                        "use a `let`/`var` local and cross task boundaries with a proof (`move`/send), or guard shared updates with `managed(\"why this is safe\", \"mutex\"|\"atomic\"|\"actor\")`".to_string(),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E3006", self.path.clone(), t.span, msg)
                            .with_notes(notes),
                    );
                    true
                }
                None => false,
            })
            .collect();

        for v in values {
            self.mark_move_in_value(st, v, format!("the move-assign `{}`", v.render()));
        }

        // A3: an index-assign on a loaned base path is a potential INSERT
        for (i, t) in targets.iter().enumerate() {
            if is_const_write[i] {
                continue;
            }
            if matches!(t.kind, PlaceKind::Index { .. }) {
                if let Some(path) = walk::place_path(t) {
                    self.emit_a3_if_live(st, &path, t.span, idx, "insert");
                }
            }
        }

        // A6: a borrow stored into a struct field (single, parallel, or multi-return)
        if targets.len() == values.len() {
            for (i, (t, v)) in targets.iter().zip(values.iter()).enumerate() {
                if is_const_write[i] {
                    continue;
                }
                if matches!(t.kind, PlaceKind::Field { .. }) {
                    let is_borrow = as_addr_of(v).is_some()
                        || matches!(self.shape_of_expr(st, v), TypeShape::BorrowHandle(_));
                    if is_borrow {
                        self.emit_a6_field_borrow_error(t);
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            for (t, shape) in targets.iter().zip(shapes.iter()) {
                if matches!(t.kind, PlaceKind::Field { .. })
                    && matches!(shape, TypeShape::BorrowHandle(_))
                {
                    self.emit_a6_field_borrow_error(t);
                }
            }
        }

        // N1: `x = nil` / `s.f = nil` / `m[k] = nil` against a known
        // non-nilable target is a compile error (HYB-E4001). An unresolvable
        // target stays silent here — no boundary is involved; N1 fires only
        // where the type is known.
        for (i, (t, v)) in targets.iter().zip(values.iter()).enumerate() {
            if is_const_write[i] {
                continue;
            }
            if Checker::is_nil_literal(v) {
                if let Some(shape) = self.place_shape(st, t) {
                    if !matches!(shape, TypeShape::Unknown)
                        && !Checker::is_nilable_shape(&shape)
                    {
                        self.emit_nil_type_error(
                            &shape,
                            v.span(),
                            &format!("the assignment target `{}`", t.render()),
                        );
                    }
                }
            }
            // N1 for composite literals: a rebind whose RHS is a literal
            // with nested nils (`j = jobT{m: nil}`) carries its own type
            // context (HYB-E4001).
            self.check_composite_nils(st, v);
        }

        // whole-binding rebind: `rows = append(rows, ...)`, parallel
        // `m2, x = m, 1`, and multi-value unpack `n, m = make_pair()`.
        if op == AssignOp::Assign {
            if values.len() == targets.len() {
                // Evaluate all RHS shapes, types, and alias sources before
                // mutating target bindings so parallel swaps/rebinds read the
                // pre-assignment environment.
                type RhsInfo = (TypeShape, Option<hy_syntax::TypeExpr>, Option<String>);
                let rhs_info: Vec<RhsInfo> =
                    values
                        .iter()
                        .map(|v| {
                            let shape = self.shape_of_expr(st, v);
                            let ty = self.ty_of_expr(st, v);
                            let non_owned = match crate::model::unwrap_parens(v) {
                                Expr::Ident { name: src, .. } => self
                                    .lookup_binding(st, src)
                                    .and_then(|b| b.non_owned_origin.clone()),
                                other => move_operand_ident(other)
                                    .and_then(|src| self.lookup_binding(st, &src))
                                    .and_then(|b| b.non_owned_origin.clone()),
                            };
                            (shape, ty, non_owned)
                        })
                        .collect();

                for (i, ((t, v), (shape, ty, non_owned))) in targets
                    .iter()
                    .zip(values.iter())
                    .zip(rhs_info)
                    .enumerate()
                {
                    if is_const_write[i] {
                        continue;
                    }
                    if let PlaceKind::Ident(name) = &t.kind {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                                b.shape = shape;
                            }
                            if ty.is_some() || b.ty.is_none() {
                                b.ty = ty;
                            }
                            b.moved = None; // a fresh value re-owns the binding
                            b.non_owned_origin = non_owned;
                        }
                        // a rebind re-owns the binding: drop the old alias and
                        // may-share edges, then re-record from the new value (#27)
                        st.aliases.remove(name);
                        st.share_clear(name);
                        self.record_may_share(st, name, v);
                        // rebind may create a fresh alias — ident copies AND
                        // call results that hand back argument storage (#38)
                        self.record_alias(st, name, v);
                    }
                }
            } else if values.len() == 1 && targets.len() > 1 {
                let shapes = self.multi_value_shapes(st, &values[0], targets.len());
                let tys = self.multi_value_tys(st, &values[0], targets.len());
                for ((t, shape), ty) in targets.iter().zip(shapes).zip(tys) {
                    if let PlaceKind::Ident(name) = &t.kind {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                                b.shape = shape;
                            }
                            if ty.is_some() || b.ty.is_none() {
                                b.ty = ty;
                            }
                            b.moved = None;
                        }
                        st.aliases.remove(name);
                    }
                }
            }
        }
    }



    fn emit_a6_field_borrow_error(&mut self, t: &Place) {
        let msg = format!(
            "a borrow value cannot be stored in the field `{}`",
            t.render()
        );
        let notes = vec![
            "struct fields hold owned values, GC values, ids, or own-handles — never borrows (O12): a stored borrow is the shape that would force lifetime annotations".to_string(),
            "store the index or a copy and re-view through the handle at the use site (id discipline)".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3103", self.path.clone(), t.span, msg).with_notes(notes),
        );
    }

    /// The binding name at the ROOT of place `p` when that binding is a
    /// `const` (fn-local `LocalKind::Const` or a file-level const — both
    /// carry `immutable: true`, and `lookup_binding` resolves locals first,
    /// so a shadowing copy is never reported). Field/index places walk to
    /// their base: `cells[0]`, `cfg.tag`, and the plain rebind `cells = ...`
    /// all resolve through here (T22 / issue #37).
    fn const_write_root(&self, st: &FnState, p: &Place) -> Option<String> {
        match &p.kind {
            PlaceKind::Ident(name) => self
                .lookup_binding(st, name)
                .filter(|b| b.immutable)
                .map(|_| name.clone()),
            PlaceKind::Field { base, .. } | PlaceKind::Index { base, .. } => {
                self.const_write_root(st, base)
            }
            _ => None,
        }
    }

    /// The shape of an assignment place, when it resolves (drives the N1
    /// nil checks on assignment targets).
    fn place_shape(&self, st: &FnState, p: &Place) -> Option<TypeShape> {
        match &p.kind {
            PlaceKind::Ident(name) => self.lookup_binding(st, name).map(|b| b.shape.clone()),
            PlaceKind::Field { base, name } => {
                let base_shape = self.place_shape(st, base)?;
                Some(self.field_shape(&base_shape, name))
            }
            PlaceKind::Index { base, .. } => match self.place_shape(st, base)? {
                TypeShape::Map(_, v) => Some(*v),
                TypeShape::Slice(e) | TypeShape::Array(e) => Some(*e),
                _ => None,
            },
            _ => None,
        }
    }

    // -----------------------------------------------------------------------
    // Returns (LOAN-ESCAPE + move marking)
    // -----------------------------------------------------------------------

    pub(crate) fn process_return(&mut self, st: &mut FnState, values: &[Expr]) {
        // N1: `return nil` under a non-nilable declared result is a compile
        // error (HYB-E4001). `error` results resolve to Unknown today, so
        // `return 0, nil` stays legal (the nil's target did not resolve).
        if values.len() == st.result_shapes.len() && !st.result_shapes.is_empty() {
            for (v, shape) in values.iter().zip(st.result_shapes.iter()) {
                if Checker::is_nil_literal(v)
                    && !matches!(shape, TypeShape::Unknown)
                    && !Checker::is_nilable_shape(shape)
                {
                    self.emit_nil_type_error(shape, v.span(), "a declared result of this function");
                }
            }
        }
        for v in values {
            // N1 for composite literals: a returned literal with nested
            // nils (`return jobT{m: nil}`) carries its own type context
            // (HYB-E4001).
            self.check_composite_nils(st, v);
            // O2/LOAN-ESCAPE: a borrow returned from a function
            if as_addr_of(v).is_some() || contains_addr_of_in_literal(v) {
                let msg = "a borrow cannot be returned from a function".to_string();
                let notes = vec![
                    format!(
                        "`{}` is a loan whose window ends when this function exits — Tier 0 keeps every loan function-local (O2): that is why there are no lifetime annotations to write",
                        v.render()
                    ),
                    "make the mutation a method on the owner so the loan stays in one frame, return the index and let the caller do the access, or return a copy (Go-heap reads are copies, O9)".to_string(),
                    "a lifetime parameter is NOT offered on purpose: the language has none".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3104", self.path.clone(), v.span(), msg)
                        .with_notes(notes),
                );
            } else {
                self.mark_move_in_value(st, v, format!("the move-return `{}`", v.render()));
            }
        }
    }
}

/// Does this literal contain an address-of at its top level (field values,
/// map entries, slice items — recursively through nested literals only)?
fn contains_addr_of_in_literal(e: &Expr) -> bool {
    if as_addr_of(e).is_some() {
        return true;
    }
    match e {
        Expr::StructLit { fields, .. } => {
            fields.iter().any(|f| contains_addr_of_in_literal(&f.value))
        }
        Expr::MapLit { entries, .. } => entries
            .iter()
            .any(|(k, v)| contains_addr_of_in_literal(k) || contains_addr_of_in_literal(v)),
        Expr::SliceLit { items, .. } => items.iter().any(contains_addr_of_in_literal),
        Expr::Paren { inner, .. } => contains_addr_of_in_literal(inner),
        _ => false, // do not look through calls/indices — literal contents only
    }
}

// ---------------------------------------------------------------------------
// File-level A6: struct field declarations (corpus 11's violation site)
// ---------------------------------------------------------------------------

impl<'a> Checker<'a> {
    pub(crate) fn check_struct_decls(&mut self) {
        let decls: Vec<(String, Vec<(String, hy_syntax::TypeExpr)>)> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                hy_syntax::Decl::Struct(s) => Some((
                    s.name.clone(),
                    s.fields
                        .iter()
                        .map(|f| (f.name.clone(), f.ty.clone()))
                        .collect(),
                )),
                _ => None,
            })
            .collect();

        for (sname, fields) in decls {
            for (fname, fty) in fields {
                let shape = self.shape_of_type(&fty);
                match &shape {
                    TypeShape::BorrowHandle(_) => {
                        let msg = format!(
                            "struct field `{}.{}` has borrow-handle type `{}`",
                            sname,
                            fname,
                            fty.render()
                        );
                        let notes = vec![
                            "fields hold owned values, GC values, ids, or own-handles — never borrows (O12): a stored borrow is the shape that would force lifetime annotations".to_string(),
                            "process the element inside the loop body (the loan dies at each iteration's last use), or store the index or a copy and re-view through the handle later".to_string(),
                        ];
                        let span = fty.span();
                        self.diags.push(
                            Diagnostic::error("HYB-E3103", self.path.clone(), span, msg)
                                .with_notes(notes),
                        );
                    }
                    TypeShape::Pointer(inner) if !matches!(**inner, TypeShape::Atomic) => {
                        // O12's own example (`cur &node`): pointer fields are
                        // borrows — store an id or the value instead.
                        let msg = format!(
                            "struct field `{}.{}` has pointer type `{}` — fields never hold borrows",
                            sname,
                            fname,
                            fty.render()
                        );
                        let notes = vec![
                            "a pointer field is a stored borrow (O12): store an id/index and re-borrow at the use site, or store the value".to_string(),
                        ];
                        let span = fty.span();
                        self.diags.push(
                            Diagnostic::error("HYB-E3103", self.path.clone(), span, msg)
                                .with_notes(notes),
                        );
                    }
                    _ => {}
                }
            }
        }
    }

    /// `var`/`let` statements are Locals; file-level const inference:
    pub(crate) fn const_binding(&self, c: &hy_syntax::ConstDecl) -> Binding {
        let shape = match &c.ty {
            Some(t) => self.shape_of_type(t),
            None => {
                let empty = FnState::default();
                self.shape_of_expr(&empty, &c.value)
            }
        };
        let ty = match &c.ty {
            Some(t) => Some(t.clone()),
            None => {
                let empty = FnState::default();
                self.ty_of_expr(&empty, &c.value)
            }
        };
        Binding {
            shape,
            ty,
            immutable: true, // class 4: const values are immutable-by-type
            managed_taint: false,
            moved: None,
            non_owned_origin: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_file, Verdict};

    fn check(src: &str) -> crate::CheckResult {
        let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
        check_file(&ast)
    }

    #[test]
    fn multi_return_assign_preserves_second_result_shape_and_rejects_spawn() {
        let r = check(
            "fn make_pair() (int, map[string]int) {\n\
               m := map[string]int{\"init\": 0}\n\
               return 1, m\n\
             }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               n := 0\n\
               n, m = make_pair()\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("map[string]int"));
    }

    #[test]
    fn multi_return_define_tracks_reference_alias_and_freezes_on_move() {
        let r = check(
            "import \"fmt\"\n\
             fn make_pair() (int, map[string]int) {\n\
               m := map[string]int{\"init\": 0}\n\
               return 1, m\n\
             }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               n, m := make_pair()\n\
               m2 := m\n\
               task worker(move m)\n\
               m2[\"main\"] = n\n\
               fmt.println(m2[\"main\"])\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn parallel_assign_tracks_reference_alias_and_freezes_on_move() {
        let r = check(
            "import \"fmt\"\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               m2 := map[string]int{\"other\": 0}\n\
               ok := false\n\
               m2, ok = m, true\n\
               task worker(move m)\n\
               if ok {\n\
                 m2[\"main\"] = 1\n\
               }\n\
               fmt.println(m2[\"main\"])\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn parallel_assign_field_borrow_rejects_a6() {
        let r = check(
            "import \"serde_json\"\n\
             struct holder {\n\
               v serde_json.value\n\
               ok bool\n\
             }\n\
             fn main() {\n\
               doc := serde_json.parse(\"[1, 2]\")\n\
               h := holder{v: serde_json.parse(\"[]\"), ok: false}\n\
               h.v, h.ok = doc.array(), true\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3103");
    }

    #[test]
    fn parallel_define_map_slot_loan_rejects_a3_insert() {
        let r = check(
            "import \"fmt\"\n\
             struct svc { count int }\n\
             fn main() {\n\
               m := map[string]svc{\"a\": svc{count: 1}}\n\
               s, ok := &m[\"a\"], true\n\
               m[\"b\"] = svc{count: 2}\n\
               if ok {\n\
                 fmt.println(s.count)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3102");
    }

    #[test]
    fn multi_return_and_parallel_assign_moved_without_post_use_accepts() {
        let r = check(
            "import \"fmt\"\n\
             fn make_pair() (int, map[string]int) {\n\
               m := map[string]int{\"init\": 0}\n\
               return 1, m\n\
             }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               n := 0\n\
               n, m = make_pair()\n\
               task worker(move m)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }
}

