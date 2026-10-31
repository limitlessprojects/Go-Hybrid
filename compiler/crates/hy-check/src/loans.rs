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
    as_addr_of, collect_addr_of_roots, collect_ref_roots, move_operand_ident, unwrap_parens, Binding, Checker, FnState,
    Loan, LoanTarget, MoveKind, TypeShape,
};
use crate::shape::receiver_type_name;
use crate::walk::{self, CallRef};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Block, Expr, FnDecl, LocalDecl, LocalKind, Place, PlaceKind, Stmt};
use std::collections::{HashMap, HashSet};

/// Read-only / non-map-mutating builtin call targets that can never insert or
/// delete on a map argument — the A3 callee scan fails closed on everything
/// else (#111). (`delete` is checked explicitly by `check_delete_conflict`.)
const A3_PURE_FNS: &[&str] = &[
    "len", "cap", "print", "println", "append", "copy", "make", "new", "close", "panic", "recover",
];
/// Read-only packages (`fmt.println(m)`, `strings.split(...)`) — their
/// methods receive values without storing to them (#111).
const A3_PURE_PKGS: &[&str] = &["fmt", "strings", "strconv", "math", "unicode"];

/// First segment of a dotted path (`e.services` -> `e`): the binding whose
/// storage the path reaches through.
fn path_root(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

/// #111 A3 peer connectivity: an event path invalidates a loan on another
/// path when the two ROOTS are the same binding or connected through the
/// may-share graph (`alias := m`, shallow copies, closures) or the
/// single-parent alias chain. Used instead of string equality so an insert
/// through a peer of the loaned map fires the same E3102 as a direct one.
fn a3_paths_conflict(st: &FnState, loan_path: &str, event_path: &str) -> bool {
    let lr = path_root(loan_path);
    let er = path_root(event_path);
    if lr == er {
        return true;
    }
    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(lr.to_string());
    let mut stack = vec![lr.to_string()];
    while let Some(x) = stack.pop() {
        if x == er {
            return true;
        }
        if let Some(p) = st.aliases.get(&x) {
            if seen.insert(p.clone()) {
                stack.push(p.clone());
            }
        }
        for (k, v) in &st.aliases {
            if v == &x && seen.insert(k.clone()) {
                stack.push(k.clone());
            }
        }
        if let Some(peers) = st.may_share.get(&x) {
            for p in peers {
                if seen.insert(p.clone()) {
                    stack.push(p.clone());
                }
            }
        }
    }
    false
}

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
        root_span: Span,
        method: String,
        recv: TypeShape,
    },
    /// `name(...)` — a plain function call (or builtin).
    Fn {
        name: String,
    },
    None,
}

#[derive(Clone)]
enum CalleeSlotSource<'c> {
    Arg(&'c Expr),
    OuterBinding(String),
}

fn root_ident_span(expr: &Expr) -> Option<Span> {
    match expr {
        Expr::Ident { span, .. } => Some(*span),
        Expr::Field { base, .. } | Expr::Index { base, .. } | Expr::Slice { base, .. } => {
            root_ident_span(base)
        }
        Expr::Paren { inner, .. } => root_ident_span(inner),
        _ => None,
    }
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
                    let root_span = root_ident_span(base).unwrap_or_else(|| base.span());
                    return Resolved::Method {
                        root: root.to_string(),
                        root_span,
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
        stmt_consumes: &mut Vec<(Span, Span)>,
    ) {
        match self.resolve_call(st, cr) {
            Resolved::Package { pkg, method } => {
                // T22: a package call that mutates an argument in place
                // (`sort.ints(cells)`, `sort.slice(...)`, etc.) is a write to
                // that argument's `const` root (`HYB-E3006`).
                self.check_call_const_writes(st, cr, &HashSet::new());
                match foreign::package_mode(&pkg, &method) {
                    CallMode::Borrow => {
                        // a borrow-mode package call lends its handle-typed args
                        // for the call (`serde_json.to_string_pretty(doc)`) or
                        // across the whole loop when iterating a view
                        self.lend_handle_args(st, cr, idx, loop_end);
                    }
                    CallMode::Mutate => {
                        for a in cr.args {
                            if let Some(root) = a.value.root_ident() {
                                let arg_is_handle = self.shape_of_expr(st, &a.value).is_handle();
                                let active_handle = self
                                    .lookup_binding(st, root)
                                    .map(|b| (b.shape.is_handle() || arg_is_handle) && b.moved.is_none())
                                    .unwrap_or(false);
                                if active_handle {
                                    self.check_a2_conflict(st, cr, root, idx);
                                }
                            }
                        }
                    }
                    CallMode::Consume => {
                        for a in cr.args {
                            if let Some(root) = a.value.root_ident() {
                                let arg_is_handle = self.shape_of_expr(st, &a.value).is_handle();
                                let active_handle = self
                                    .lookup_binding(st, root)
                                    .map(|b| (b.shape.is_handle() || arg_is_handle) && b.moved.is_none())
                                    .unwrap_or(false);
                                if active_handle {
                                    let rspan = root_ident_span(&a.value).unwrap_or(a.span);
                                    self.mark_move(
                                        st,
                                        root,
                                        cr.span,
                                        format!("the consuming call `{}`", cr.callee.render()),
                                        MoveKind::Consumed,
                                    );
                                    stmt_consumes.push((rspan, cr.span));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Resolved::Method {
                root,
                root_span,
                method,
                recv,
            } => {
                self.check_call_const_writes(st, cr, &HashSet::new());
                self.record_callee_param_stores(st, cr);
                let Some(tname) = receiver_type_name(&recv) else {
                    self.check_a3_method_call(st, cr, idx, &root, &recv, &method);
                    return;
                };
                // #2200: a handle whose binding is vendored in `runtime/go/`
                // has a FINITE method surface, and Go rejects every other name
                // at `go build` (`doc.Get undefined (type jsonbinding.Value has
                // no field or method Get)`). Fail closed here rather than let
                // an unbuildable program pass `hyb check`. Types outside the
                // modelled surface keep the old fail-open behaviour.
                if let Some(surface) = foreign::foreign_method_surface(&tname) {
                    if !surface.contains(&method.as_str()) {
                        let list = surface.join(", ");
                        self.diags.push(
                            Diagnostic::error(
                                "HYB-E1002",
                                self.path.clone(),
                                cr.span,
                                format!(
                                    "`{}` has no method `{}` — Go refuses `{} undefined (type {} has no field or method {})` (HYB-E1002)",
                                    tname,
                                    method,
                                    cr.callee.render(),
                                    tname,
                                    method
                                ),
                            )
                            .with_notes(vec![
                                format!("`{}` is a foreign handle; its methods are `{}`", tname, list),
                                format!(
                                    "package-level names are not methods — write `{}.{}` instead of `{}.{}()`",
                                    tname.split('.').next().unwrap_or(&tname),
                                    method,
                                    root,
                                    method
                                ),
                            ]),
                        );
                        return;
                    }
                }
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
                        stmt_consumes.push((root_span, cr.span));
                    }
                    _ => {}
                }
            }
            Resolved::Fn { name } => {
                // `copy(dst, src)` (#178): an element-wise slice copy
                // duplicates the element *references* when the element type
                // carries mutable GC storage, so the destination and source
                // share it afterwards — a later `move` of either must freeze
                // the other (O5.2). Primitive-element slices share nothing.
                if name == "copy" {
                    self.record_copy_may_share(st, cr);
                }
                if name == "delete" || name == "clear" {
                    // A3: map delete / clear is a map mutation
                    self.check_delete_conflict(st, cr, idx);
                }
                if matches!(name.as_str(), "delete" | "copy" | "clear" | "append") {
                    // T22 (#123/#188): a delete, copy, or clear mutates its first
                    // argument — a `const` root is frozen like an index write.
                    // `append` writes in place into the destination's spare
                    // capacity, so a reslice of a `const` is frozen too.
                    self.check_delete_const(st, cr, &HashSet::new());
                }
                self.check_call_const_writes(st, cr, &HashSet::new());
                self.record_callee_param_stores(st, cr);
                if self.fns.contains_key(&name) {
                    // A3 #111: the callee may insert/delete through the
                    // argument — that event runs inside THIS call site's
                    // window when the call is within it.
                    self.check_a3_call_conflict(st, cr, idx, &name);
                    // A7 borrow-by-default: a handle passed to a Hybrid
                    // function is lent for the call only
                    self.lend_handle_args(st, cr, idx, None);
                } else {
                    let resolved_callee = crate::freeze::resolve_alias(st, &name);
                    if st.closure_mutates.contains_key(&name)
                        || st.closure_mutates.contains_key(&resolved_callee)
                    {
                        // A3 #111 form 4: a local closure whose body mutates a
                        // captured binding — event attributed to this call site.
                        self.check_a3_closure_call(st, cr, idx, &name);
                    } else if !A3_PURE_FNS.contains(&name.as_str()) && name != "delete" {
                        // Unresolved callee (e.g. a `fn(...)` parameter or local
                        // function variable): fail-closed on any map argument
                        // that conflicts with a live slot loan.
                        self.check_a3_unknown_call_args(st, cr, idx);
                    }
                }
            }
            Resolved::None => {
                self.check_call_const_writes(st, cr, &HashSet::new());
                self.record_callee_param_stores(st, cr);
                self.check_a3_inline_or_unknown_call(st, cr, idx);
            }
        }
    }

    /// HYB-E3101 (A2): a mutating-mode call conflicts with any live
    /// borrow-mode loan on the same handle (or an alias / may-share peer).
    fn check_a2_conflict(&mut self, st: &FnState, cr: &CallRef, root: &str, idx: usize) {
        let conflict = st.loans.iter().find(|l| {
            matches!(
                &l.target,
                LoanTarget::Handle { handle } if a3_paths_conflict(st, handle, root)
            ) && l.created <= idx
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

    /// Handle-typed arguments of a call are lent for the call (call-scoped) or
    /// across the whole loop when the call is a for-loop ITERATE expression.
    fn lend_handle_args(
        &self,
        st: &mut FnState,
        cr: &CallRef,
        idx: usize,
        loop_end: Option<usize>,
    ) {
        let last = loop_end.unwrap_or(idx);
        for a in cr.args {
            if let Some(name) = a.value.root_ident() {
                let arg_is_handle = self.shape_of_expr(st, &a.value).is_handle();
                if let Some(b) = self.lookup_binding(st, name) {
                    if (b.shape.is_handle() || arg_is_handle) && b.moved.is_none() {
                        st.loans.push(Loan {
                            target: LoanTarget::Handle {
                                handle: name.to_string(),
                            },
                            created: idx,
                            last,
                            span: a.span,
                            desc: format!(
                                "the argument `{}` lent to `{}`{}",
                                name,
                                cr.callee.render(),
                                if loop_end.is_some() {
                                    " (live across the whole loop)"
                                } else {
                                    " (A7 borrow-by-default)"
                                }
                            ),
                        });
                    }
                }
            }
        }
    }

    /// A3 via `delete(m, k)` or `clear(m)`: a delete/clear on a loaned base
    /// path inside the window invalidates the slot loan.
    fn check_delete_conflict(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        let Some(first) = cr.args.first() else { return };
        let verb = match crate::model::unwrap_parens(cr.callee) {
            Expr::Ident { name, .. } if name == "clear" => "clear",
            _ => "delete",
        };
        if let Some(path) = walk::expr_path(&first.value) {
            self.emit_a3_if_live(st, &path, cr.span, idx, verb);
        } else {
            let mut roots = Vec::new();
            collect_ref_roots(&first.value, &mut roots);
            for r in roots {
                self.emit_a3_if_live(st, &r, cr.span, idx, verb);
            }
        }
    }

    /// Compute the effective inclusive end index of a loan window, extending
    /// across any enclosing loop whose body uses the loan (`l.created < lp_idx`
    /// and `l.last` falls inside `lp_idx..=lp_end`).
    pub(crate) fn effective_loan_last(&self, st: &FnState, l: &Loan) -> usize {
        let mut last = l.last;
        for (&lp_idx, &lp_end) in &st.loop_end {
            if l.created < lp_end && last >= lp_idx && last <= lp_end {
                last = last.max(lp_end);
            }
        }
        last
    }

    pub(crate) fn emit_a3_if_live_pub(
        &mut self,
        st: &FnState,
        path: &str,
        span: Span,
        idx: usize,
        verb: &str,
    ) {
        self.emit_a3_if_live(st, path, span, idx, verb);
    }

    /// Extend any live `LoanTarget::MapSlot` window when a slot-loan binding
    /// (or an existing alias of one) is copied into `alias_name` (`p := s` or `p = s`).
    pub(crate) fn extend_map_slot_loan_to_alias(
        &self,
        st: &mut FnState,
        alias_name: &str,
        val: &Expr,
        idx: usize,
    ) {
        let Expr::Ident { name: src, .. } = unwrap_parens(val) else {
            return;
        };
        let alias_last = st.last_use.get(alias_name).copied().unwrap_or(idx).max(idx);
        let resolved_src = crate::freeze::resolve_alias(st, src);
        for loan in &mut st.loans {
            if let LoanTarget::MapSlot { binding, .. } = &loan.target {
                if (binding == src || *binding == resolved_src) && loan.created <= idx {
                    loan.last = loan.last.max(alias_last);
                }
            }
        }
    }

    /// Shared A3 emitter for index-assign inserts and `delete` calls.
    /// Matching is peer-aware (#111): an event on `alias` conflicts with a
    /// loan on `m` when the roots share storage.
    fn emit_a3_if_live(&mut self, st: &FnState, path: &str, span: Span, idx: usize, verb: &str) {
        let Some(loan) = st.loans.iter().find(|l| {
            let eff_last = self.effective_loan_last(st, l);
            matches!(&l.target, LoanTarget::MapSlot { path: p, .. }
                if a3_paths_conflict(st, p, path))
                && l.created <= idx
                && idx <= eff_last
        }) else {
            return;
        };
        let eff_last = self.effective_loan_last(st, loan);
        let (binding, loan_desc, loan_span, loan_last) = match &loan.target {
            LoanTarget::MapSlot { binding, .. } => {
                (binding.clone(), loan.desc.clone(), loan.span, eff_last)
            }
            _ => unreachable!("filtered above"),
        };
        let msg = format!("map {} invalidates the live slot loan `{}`", verb, binding);
        // One invalidation, one diagnostic: #549's dedicated detectors
        // (delete/clear/copy) and pr-242's generic mutator scan can both
        // fire for the SAME call site - same loan, same span. The first,
        // most specific wording wins; a duplicate would only bury the
        // signal the corpus count pins.
        if self.diags.iter().any(|d| {
            d.code == "HYB-E3102"
                && d.span == span
                && d.message.contains(&format!("loan `{}`", binding))
        }) {
            return;
        }
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

    /// O8 uniqueness: a live slot loan is the only access path to its slot.
    /// The emitter lowers the loan to copy-out / store-back, so a second loan
    /// on (or a field store through) a slot that may be the same slot is
    /// overwritten by, or overwrites, the loan's store-back. `owner` is the
    /// binding being (re)loaned, whose own earlier loan does not conflict.
    #[allow(clippy::too_many_arguments)]
    fn emit_o8_alias_if_live(
        &mut self,
        st: &FnState,
        path: &str,
        key: Option<&str>,
        owner: Option<&str>,
        span: Span,
        idx: usize,
        what: &str,
    ) {
        let Some(loan) = st.loans.iter().find(|l| {
            matches!(&l.target, LoanTarget::MapSlot { binding, path: p, key: k }
                if owner != Some(binding.as_str())
                    && a3_paths_conflict(st, p, path)
                    && !matches!((k.as_deref(), key), (Some(a), Some(b)) if a != b))
                && l.created <= idx
                && idx <= l.last
        }) else {
            return;
        };
        let LoanTarget::MapSlot { binding, .. } = &loan.target else {
            unreachable!("filtered above");
        };
        let msg = format!(
            "{} on `{}` aliases the live slot loan `{}`",
            what, path, binding
        );
        let notes = vec![
            format!(
                "{} ({}:{}) is a unique loan on a slot of `{}`, live from creation to its last use (O8, non-lexical window ending at statement {})",
                loan.desc, self.path, loan.span.start, path, loan.last
            ),
            format!(
                "the loan lowers to copy-out / store-back, so this access and `{}` would silently overwrite each other — finish using `{}` first, or mutate the slot only through `{}`",
                binding, binding, binding
            ),
        ];
        self.diags
            .push(Diagnostic::error("HYB-E3102", self.path.clone(), span, msg).with_notes(notes));
    }

    // -----------------------------------------------------------------------
    // #111: A3 events that cross frames, aliases, and closure bodies
    // -----------------------------------------------------------------------

    /// #111 forms 1-2: a top-level fn call inside a live slot-loan window
    /// whose argument IS (or shares storage with) the loaned map invalidates
    /// the window when the callee mutates that parameter — the insert/delete
    /// executes while the loan is live even though it is spelled in the
    /// callee's frame.
    fn check_a3_call_conflict(&mut self, st: &FnState, cr: &CallRef, idx: usize, name: &str) {
        // Collect the live loan paths first so the `self.fns` borrow below
        // does not overlap the `&mut self` emit afterwards.
        let live: Vec<String> = st
            .loans
            .iter()
            .filter_map(|l| {
                let eff_last = self.effective_loan_last(st, l);
                match &l.target {
                    LoanTarget::MapSlot { path, .. } if l.created <= idx && idx <= eff_last => {
                        Some(path.clone())
                    }
                    _ => None,
                }
            })
            .collect();
        if live.is_empty() {
            return;
        }
        let mut conflict_paths: Vec<String> = Vec::new();
        for (i, a) in cr.args.iter().enumerate() {
            let mut aroots = Vec::new();
            collect_ref_roots(&a.value, &mut aroots);
            let arg_path_opt = walk::expr_path(&a.value);
            for arg_path in arg_path_opt.into_iter().chain(aroots) {
                if !live.iter().any(|p| a3_paths_conflict(st, p, &arg_path)) {
                    continue;
                }
                let Some(decl) = self.fns.get(name) else {
                    continue;
                };
                match decl.params.get(i) {
                    Some(p) => {
                        if self.fn_mutates_param(decl, &p.name, 0) {
                            conflict_paths.push(arg_path);
                        }
                    }
                    // argument beyond the signature — fail closed
                    None => conflict_paths.push(arg_path),
                }
            }
        }
        let verb = format!("mutation via `{}`", cr.callee.render());
        for p in conflict_paths {
            self.emit_a3_if_live(st, &p, cr.span, idx, &verb);
        }
    }

    /// #111 form 4: a local closure invoked inside a live slot-loan window
    /// whose recorded mutation roots reach the loaned storage.
    fn check_a3_closure_call(&mut self, st: &FnState, cr: &CallRef, idx: usize, name: &str) {
        let resolved = crate::freeze::resolve_alias(st, name);
        let Some(roots) = st
            .closure_mutates
            .get(name)
            .or_else(|| st.closure_mutates.get(&resolved))
        else {
            return;
        };
        if roots.is_empty() {
            return;
        }
        let verb = format!("mutation via `{}`", cr.callee.render());
        for l in &st.loans {
            let LoanTarget::MapSlot { path, .. } = &l.target else {
                continue;
            };
            let eff_last = self.effective_loan_last(st, l);
            if !(l.created <= idx && idx <= eff_last) {
                continue;
            }
            if roots.iter().any(|r| a3_paths_conflict(st, path, r)) {
                self.emit_a3_if_live(st, path, cr.span, idx, &verb);
                return;
            }
        }
    }

    /// A3 check for user-defined method calls (`e.bump()`), closure-field calls,
    /// and unknown method receivers while a map-slot loan is live.
    fn check_a3_method_call(
        &mut self,
        st: &FnState,
        cr: &CallRef,
        idx: usize,
        root: &str,
        recv: &TypeShape,
        method: &str,
    ) {
        let live: Vec<String> = st
            .loans
            .iter()
            .filter_map(|l| {
                let eff_last = self.effective_loan_last(st, l);
                match &l.target {
                    LoanTarget::MapSlot { path, .. } if l.created <= idx && idx <= eff_last => {
                        Some(path.clone())
                    }
                    _ => None,
                }
            })
            .collect();
        if live.is_empty() {
            return;
        }
        let recv_sname = match recv {
            TypeShape::Struct(Some(s)) => Some(s.clone()),
            TypeShape::Pointer(inner) => match &**inner {
                TypeShape::Struct(Some(s)) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        };
        let verb = format!("mutation via `{}`", cr.callee.render());
        let callee_path = walk::expr_path(cr.callee).unwrap_or_else(|| root.to_string());
        if let Some(sname) = recv_sname {
            let found_decl = self.file.decls.iter().find_map(|d| match d {
                hy_syntax::Decl::Fn(f) if f.name == method => {
                    let r = f.receiver.as_ref()?;
                    let b = Self::base_named_type(&r.ty)?;
                    (b == sname).then_some(f)
                }
                _ => None,
            });
            if let Some(decl) = found_decl {
                if let Some(r) = &decl.receiver {
                    if live
                        .iter()
                        .any(|p| a3_paths_conflict(st, p, root) || a3_paths_conflict(st, p, &callee_path))
                        && self.fn_mutates_param(decl, &r.name, 0)
                    {
                        for p in &live {
                            if a3_paths_conflict(st, p, root)
                                || a3_paths_conflict(st, p, &callee_path)
                            {
                                self.emit_a3_if_live(st, p, cr.span, idx, &verb);
                                return;
                            }
                        }
                    }
                }
                for (i, a) in cr.args.iter().enumerate() {
                    let Some(arg_path) = walk::expr_path(&a.value) else {
                        continue;
                    };
                    if !live.iter().any(|p| a3_paths_conflict(st, p, &arg_path)) {
                        continue;
                    };
                    let mutates = match decl.params.get(i) {
                        Some(p) => self.fn_mutates_param(decl, &p.name, 0),
                        None => true,
                    };
                    if mutates {
                        self.emit_a3_if_live(st, &arg_path, cr.span, idx, &verb);
                        return;
                    }
                }
                return;
            }
        }
        for p in &live {
            if a3_paths_conflict(st, p, root) || a3_paths_conflict(st, p, &callee_path) {
                self.emit_a3_if_live(st, p, cr.span, idx, &verb);
                return;
            }
        }
        self.check_a3_unknown_call_args(st, cr, idx);
    }

    /// Fail-closed A3 check for calls to unresolved/parameter functions passing
    /// a loaned map or struct containing a loaned map.
    fn check_a3_unknown_call_args(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        let live: Vec<String> = st
            .loans
            .iter()
            .filter_map(|l| {
                let eff_last = self.effective_loan_last(st, l);
                match &l.target {
                    LoanTarget::MapSlot { path, .. } if l.created <= idx && idx <= eff_last => {
                        Some(path.clone())
                    }
                    _ => None,
                }
            })
            .collect();
        if live.is_empty() {
            return;
        }
        let verb = format!("mutation via `{}`", cr.callee.render());
        for a in cr.args {
            let Some(arg_path) = walk::expr_path(&a.value) else {
                continue;
            };
            if live.iter().any(|p| a3_paths_conflict(st, p, &arg_path)) {
                self.emit_a3_if_live(st, &arg_path, cr.span, idx, &verb);
                return;
            }
        }
    }

    /// A3 check for inline closure calls (`(fn() { m["b"] = ... })()`) or
    /// other non-identifier call expressions.
    fn check_a3_inline_or_unknown_call(&mut self, st: &FnState, cr: &CallRef, idx: usize) {
        if let Expr::FnLit {
            params,
            results,
            body,
            ..
        } = unwrap_parens(cr.callee)
        {
            let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
            bound.extend(results.iter().filter_map(|r| r.name.clone()));
            let roots = self.collect_mutation_roots(st, body, &bound);
            if !roots.is_empty() {
                let verb = format!("mutation via `{}`", cr.callee.render());
                for l in &st.loans {
                    let LoanTarget::MapSlot { path, .. } = &l.target else {
                        continue;
                    };
                    let eff_last = self.effective_loan_last(st, l);
                    if !(l.created <= idx && idx <= eff_last) {
                        continue;
                    }
                    if roots.iter().any(|r| a3_paths_conflict(st, path, r)) {
                        self.emit_a3_if_live(st, path, cr.span, idx, &verb);
                        return;
                    }
                }
            }
        }
        self.check_a3_unknown_call_args(st, cr, idx);
    }

    /// Does `decl` mutate its parameter `param` (map insert/delete — the
    /// only A3 events)? Bounded interprocedural scan, fail-closed past the
    /// depth bound and on unknown callees that receive the parameter (#111).
    fn fn_mutates_param(&self, decl: &FnDecl, param: &str, depth: usize) -> bool {
        if depth > 3 {
            return true; // fail closed past the bound
        }
        let Some(body) = &decl.body else {
            return false;
        };
        self.block_mutates_param(body, param, depth)
    }

    fn block_mutates_param(&self, block: &Block, param: &str, depth: usize) -> bool {
        let flat = walk::flatten(block);
        let mut alias_stack: Vec<HashSet<String>> = vec![[param.to_string()].into_iter().collect()];
        let mut locals_stack: Vec<HashSet<String>> = vec![HashSet::new()];
        for (stmt, idx) in &flat.stmts {
            if let Some(evs) = flat.events.get(idx) {
                for ev in evs {
                    match ev {
                        walk::ScopeEvent::Push => {
                            alias_stack.push(alias_stack.last().unwrap().clone());
                            locals_stack.push(HashSet::new());
                        }
                        walk::ScopeEvent::Pop => {
                            if alias_stack.len() > 1 {
                                let popped_aliases = alias_stack.pop().unwrap();
                                let popped_locals = locals_stack.pop().unwrap();
                                for a in popped_aliases {
                                    if !popped_locals.contains(&a) {
                                        alias_stack.last_mut().unwrap().insert(a);
                                    }
                                }
                            }
                        }
                        walk::ScopeEvent::MatchBind { name, subject } => {
                            let mut sroots = Vec::new();
                            collect_ref_roots(subject, &mut sroots);
                            if sroots.iter().any(|r| alias_stack.last().unwrap().contains(r)) {
                                alias_stack.last_mut().unwrap().insert(name.to_string());
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Check if stmt mutates any active alias of param
            match stmt {
                Stmt::Assign { targets, values, .. } => {
                    for t in targets {
                        if (matches!(t.kind, PlaceKind::Index { .. }) || walk::place_field_store_slot(t).is_some()) && alias_stack.last().unwrap().contains(t.root()) {
                            return true;
                        }
                    }
                    for v in values {
                        if alias_stack.last().unwrap().iter().any(|p| self.expr_fnlit_mutates_param(v, p, depth)) {
                            return true;
                        }
                    }
                }
                Stmt::Local(l) => {
                    for v in &l.values {
                        if alias_stack.last().unwrap().iter().any(|p| self.expr_fnlit_mutates_param(v, p, depth)) {
                            return true;
                        }
                    }
                }
                Stmt::Return { values, .. } => {
                    // a returned closure that mutates the param counts as a
                    // mutation of the param (the helper-returns-closure form,
                    // issue #490's make_mutator shape)
                    for v in values {
                        if alias_stack.last().unwrap().iter().any(|p| self.expr_fnlit_mutates_param(v, p, depth)) {
                            return true;
                        }
                    }
                }
                _ => {}
            }

            // Check calls in this statement
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            for cr in &calls {
                if alias_stack.last().unwrap().iter().any(|p| self.call_mutates_param(cr, p, depth)) {
                    return true;
                }
            }

            // Update aliases
            let pre_aliases = alias_stack.last().unwrap().clone();
            match stmt {
                Stmt::Local(l) => {
                    for (name, val) in l.names.iter().zip(if l.values.len() == 1 { vec![&l.values[0]; l.names.len()] } else { l.values.iter().collect() }) {
                        locals_stack.last_mut().unwrap().insert(name.clone());
                        let mut roots = Vec::new();
                        collect_ref_roots(val, &mut roots);
                        if roots.iter().any(|r| pre_aliases.contains(r)) {
                            alias_stack.last_mut().unwrap().insert(name.clone());
                        } else {
                            alias_stack.last_mut().unwrap().remove(name);
                        }
                    }
                }
                Stmt::Assign { targets, values, .. } if targets.len() == values.len() => {
                    for (t, val) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            let mut roots = Vec::new();
                            collect_ref_roots(val, &mut roots);
                            if roots.iter().any(|r| pre_aliases.contains(r)) {
                                alias_stack.last_mut().unwrap().insert(name.clone());
                            } else {
                                alias_stack.last_mut().unwrap().remove(name);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        false
    }

    /// A nested FnLit inside the callee body mutates `param` only if it
    /// captures it (a body declaring its own `param` shadows it — those
    /// references are the closure's fresh binding, not the caller's map).
    fn expr_fnlit_mutates_param(&self, e: &Expr, param: &str, depth: usize) -> bool {
        if walk::addr_of_index_base_path(e).is_some_and(|p| path_root(&p) == param) { return true; }
        match e {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                if params.iter().any(|p| p.name == param)
                    || results.iter().any(|r| r.name.as_deref() == Some(param))
                {
                    false
                } else {
                    self.block_mutates_param(body, param, depth)
                }
            }
            Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => self.expr_fnlit_mutates_param(inner, param, depth),
            Expr::Call { callee, args, .. } => {
                self.expr_fnlit_mutates_param(callee, param, depth)
                    || args
                        .iter()
                        .any(|a| self.expr_fnlit_mutates_param(&a.value, param, depth))
            }
            Expr::Index { base, index, .. } | Expr::Binary { lhs: base, rhs: index, .. } => {
                self.expr_fnlit_mutates_param(base, param, depth)
                    || self.expr_fnlit_mutates_param(index, param, depth)
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| self.expr_fnlit_mutates_param(&f.value, param, depth)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .any(|it| self.expr_fnlit_mutates_param(it, param, depth)),
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                self.expr_fnlit_mutates_param(k, param, depth)
                    || self.expr_fnlit_mutates_param(v, param, depth)
            }),
            _ => false,
        }
    }

    /// Does this call site store through / delete `param`?
    fn call_mutates_param(&self, cr: &CallRef, param: &str, depth: usize) -> bool {
        let arg_root_is_param = |a: &hy_syntax::CallArg| {
            let mut roots = Vec::new();
            collect_ref_roots(&a.value, &mut roots);
            roots.iter().any(|r| r == param)
        };
        match cr.callee {
            Expr::Ident { name, .. } => {
                if name == "delete" || name == "clear" {
                    return cr.args.first().map(&arg_root_is_param).unwrap_or(false);
                }
                if A3_PURE_FNS.contains(&name.as_str()) {
                    return false;
                }
                if let Some(decl) = self.fns.get(name) {
                    for (i, a) in cr.args.iter().enumerate() {
                        if !arg_root_is_param(a) {
                            continue;
                        }
                        match decl.params.get(i) {
                            Some(p) => {
                                if self.fn_mutates_param(decl, &p.name, depth + 1) {
                                    return true;
                                }
                            }
                            None => return true, // fail closed
                        }
                    }
                    return false;
                }
                // unknown Ident callee (closure / builtin we don't model):
                // passing the parameter fails closed
                cr.args.iter().any(&arg_root_is_param)
            }
            Expr::Field { base, .. } => {
                if let Expr::Ident { name: pkg, .. } = &**base {
                    if A3_PURE_PKGS.contains(&pkg.as_str()) {
                        return false;
                    }
                }
                cr.args.iter().any(&arg_root_is_param)
            }
            _ => cr.args.iter().any(&arg_root_is_param),
        }
    }

    /// Record (at define/rebind time) which base paths a local closure's
    /// body may mutate — consumed at the closure's call sites (#111 form 4).
    /// The closure's own params are excluded: they are fresh bindings.
    pub(crate) fn record_closure_mutations(&self, st: &mut FnState, name: &str, value: &Expr) {
        match unwrap_parens(value) {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                st.closure_defs.insert(
                    name.to_string(),
                    (params.clone(), results.clone(), body.clone()),
                );
                let typed_params: Vec<(String, Option<hy_syntax::TypeExpr>)> = params
                    .iter()
                    .map(|p| (p.name.clone(), Some(p.ty.clone())))
                    .collect();
                let typed_results: Vec<(String, Option<hy_syntax::TypeExpr>)> = results
                    .iter()
                    .map(|r| (r.name.clone().unwrap_or_default(), Some(r.ty.clone())))
                    .collect();
                st.closure_fnlits.insert(
                    name.to_string(),
                    (typed_params, typed_results, body.clone()),
                );
                let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
                bound.extend(results.iter().filter_map(|r| r.name.clone()));
                let roots = self.collect_mutation_roots(st, body, &bound);
                if !roots.is_empty() {
                    st.closure_mutates
                        .entry(name.to_string())
                        .or_default()
                        .extend(roots);
                }
                let mut cap_bound = bound;
                let mut caps = Vec::new();
                collect_capture_idents_block(body, &mut cap_bound, &mut caps);
                if let Some(&closure_last) = st.last_use.get(name) {
                    for loan in &mut st.loans {
                        if let LoanTarget::MapSlot { binding, .. } = &loan.target {
                            if caps.iter().any(|c| c == binding) {
                                loan.last = loan.last.max(closure_last);
                            }
                        }
                    }
                }
            }
            Expr::Ident { name: src, .. } => {
                let resolved = crate::freeze::resolve_alias(st, src);
                if let Some(def) = st
                    .closure_defs
                    .get(src)
                    .or_else(|| st.closure_defs.get(&resolved))
                    .cloned()
                {
                    st.closure_defs.insert(name.to_string(), def);
                }
                if let Some(existing_fnlit) = st.closure_fnlits.get(src).cloned() {
                    st.closure_fnlits.insert(name.to_string(), existing_fnlit);
                }
                let inherited = st
                    .closure_mutates
                    .get(src)
                    .or_else(|| st.closure_mutates.get(&resolved))
                    .cloned();
                if let Some(roots) = inherited {
                    if !roots.is_empty() {
                        st.closure_mutates
                            .entry(name.to_string())
                            .or_default()
                            .extend(roots);
                    }
                }
            }
            Expr::Call { callee, args, .. } => {
                if !matches!(self.shape_of_expr(st, value), TypeShape::Closure) {
                    return;
                }
                let mut mutated_roots = HashSet::new();
                if let Expr::Ident { name: fn_name, .. } = unwrap_parens(callee) {
                    if let Some(decl) = self.fns.get(fn_name) {
                        for (i, a) in args.iter().enumerate() {
                            let mutates = match decl.params.get(i) {
                                Some(p) => self.fn_mutates_param(decl, &p.name, 0),
                                None => true,
                            };
                            if mutates {
                                let mut r = Vec::new();
                                collect_ref_roots(&a.value, &mut r);
                                mutated_roots.extend(r);
                            }
                        }
                    }
                }
                if !mutated_roots.is_empty() {
                    st.closure_mutates
                        .entry(name.to_string())
                        .or_default()
                        .extend(mutated_roots);
                }
            }
            _ => {}
        }
    }

    /// Every base path this block may store through or delete: index-assign
    /// targets, `delete` first args, and (via calls) anything a called top
    /// fn mutates of its parameter, or another local closure mutates.
    /// `bound` = names shadowed by enclosing closure params (#111).
    fn collect_mutation_roots(
        &self,
        st: &FnState,
        body: &Block,
        bound: &HashSet<String>,
    ) -> HashSet<String> {
        let mut out: HashSet<String> = HashSet::new();
        let mut shadow_stack: Vec<HashSet<String>> = vec![bound.clone()];
        let mut alias_stack: Vec<std::collections::HashMap<String, String>> =
            vec![std::collections::HashMap::new()];
        let mut locals_stack: Vec<HashSet<String>> = vec![HashSet::new()];
        let mut match_aliases: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        let resolve = |raw: &str,
                       aliases: &std::collections::HashMap<String, String>,
                       shadows: &HashSet<String>|
         -> Option<String> {
            if let Some(target) = aliases.get(raw) {
                return Some(target.clone());
            }
            if shadows.contains(raw) {
                return None;
            }
            Some(raw.to_string())
        };

        let flat = walk::flatten(body);
        for (stmt, idx) in &flat.stmts {
            if let Some(evs) = flat.events.get(idx) {
                for ev in evs {
                    match ev {
                        walk::ScopeEvent::Push => {
                            shadow_stack.push(shadow_stack.last().unwrap().clone());
                            alias_stack.push(alias_stack.last().unwrap().clone());
                            locals_stack.push(HashSet::new());
                        }
                        walk::ScopeEvent::Pop => {
                            if shadow_stack.len() > 1 {
                                shadow_stack.pop();
                                let popped_aliases = alias_stack.pop().unwrap();
                                let popped_locals = locals_stack.pop().unwrap();
                                for (k, v) in popped_aliases {
                                    if !popped_locals.contains(&k) {
                                        alias_stack.last_mut().unwrap().insert(k, v);
                                    }
                                }
                            }
                        }
                        walk::ScopeEvent::MatchBind { name, subject } => {
                            let mut sroots = Vec::new();
                            collect_ref_roots(subject, &mut sroots);
                            sroots.retain(|r| !bound.contains(r));
                            let current_shadows = shadow_stack.last().unwrap();
                            let current_aliases = alias_stack.last().unwrap();
                            for sr in sroots {
                                if let Some(target) = resolve(&sr, current_aliases, current_shadows) {
                                    match_aliases.entry(name.to_string()).or_default().push(target.clone());
                                    alias_stack.last_mut().unwrap().insert(name.to_string(), target);
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }

            let current_shadows = shadow_stack.last_mut().unwrap();
            let current_aliases = alias_stack.last_mut().unwrap();
            let current_locals = locals_stack.last_mut().unwrap();

            let pre_aliases = current_aliases.clone();
            let pre_shadows = current_shadows.clone();
            match stmt {
                Stmt::Local(l) => {
                    for (name, val) in l.names.iter().zip(if l.values.len() == 1 { vec![&l.values[0]; l.names.len()] } else { l.values.iter().collect() }) {
                        current_locals.insert(name.clone());
                        let mut roots = Vec::new();
                        collect_ref_roots(val, &mut roots);
                        let resolved = roots.iter().find_map(|p| resolve(p, &pre_aliases, &pre_shadows));
                        if let Some(r) = resolved {
                            current_shadows.remove(name.as_str());
                            current_aliases.insert(name.clone(), r);
                        } else {
                            current_aliases.remove(name.as_str());
                            current_shadows.insert(name.clone());
                        }
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } if targets.len() == values.len() => {
                    for (t, val) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            let mut roots = Vec::new();
                            collect_ref_roots(val, &mut roots);
                            let resolved = roots.iter().find_map(|p| resolve(p, &pre_aliases, &pre_shadows));
                            if let Some(r) = resolved {
                                current_shadows.remove(name.as_str());
                                current_aliases.insert(name.clone(), r);
                            } else if bound.contains(name.as_str()) {
                                current_aliases.remove(name.as_str());
                                current_shadows.insert(name.clone());
                            }
                        }
                    }
                }
                Stmt::Match { subject, arms, .. } => {
                    let mut sroots = Vec::new();
                    collect_ref_roots(subject, &mut sroots);
                    sroots.retain(|r| !bound.contains(r));
                    for arm in arms {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(bname),
                            ..
                        } = &arm.pattern
                        {
                            match_aliases
                                .entry(bname.clone())
                                .or_default()
                                .extend(sroots.iter().cloned());
                        }
                    }
                }
                _ => {}
            }
            // index-assign targets
            if let Stmt::Assign { targets, .. } = stmt {
                for t in targets {
                    if matches!(t.kind, PlaceKind::Index { .. }) || walk::place_field_store_slot(t).is_some() {
                        let raw = t.root().to_string();
                        if let Some(r) = resolve(&raw, current_aliases, current_shadows) {
                            out.insert(r);
                        }
                    }
                }
            }
            // FnLit values bound/rebound in this block (captures mutated by
            // nested closures — flatten does not descend into FnLit bodies)
            let mut fnlit_values: Vec<&Expr> = Vec::new();
            match stmt {
                Stmt::Local(l) => fnlit_values.extend(l.values.iter()),
                Stmt::Assign { values, .. } => fnlit_values.extend(values.iter()),
                _ => {}
            }
            for v in fnlit_values {
                if let Some(r) = walk::addr_of_index_base_path(v).and_then(|p| resolve(path_root(&p), &pre_aliases, &pre_shadows)) { out.insert(r); }
                if let Expr::FnLit {
                    params: fp,
                    results: fr,
                    body: fb,
                    ..
                } = unwrap_parens(v)
                {
                    let mut inner_bound: HashSet<String> =
                        fp.iter().map(|p| p.name.clone()).collect();
                    inner_bound.extend(fr.iter().filter_map(|r| r.name.clone()));
                    for ir in self.collect_mutation_roots(st, fb, &inner_bound) {
                        if let Some(r) = resolve(&ir, current_aliases, current_shadows) {
                            out.insert(r);
                        }
                    }
                }
            }
            // calls in this statement
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            for cr in &calls {
                match cr.callee {
                    Expr::Ident { name, .. } => {
                        if name == "delete" || name == "clear" {
                            if let Some(first) = cr.args.first() {
                                let mut roots = Vec::new();
                                collect_ref_roots(&first.value, &mut roots);
                                for root in roots {
                                    if let Some(r) =
                                        resolve(&root, current_aliases, current_shadows)
                                    {
                                        out.insert(r);
                                    }
                                }
                            }
                            continue;
                        }
                        if A3_PURE_FNS.contains(&name.as_str()) {
                            continue;
                        }
                        if let Some(decl) = self.fns.get(name) {
                            for (i, a) in cr.args.iter().enumerate() {
                                let mut roots = Vec::new();
                                collect_ref_roots(&a.value, &mut roots);
                                for root in roots {
                                    let Some(r) =
                                        resolve(&root, current_aliases, current_shadows)
                                    else {
                                        continue;
                                    };
                                    let mutates = match decl.params.get(i) {
                                        Some(tp) => self.fn_mutates_param(decl, &tp.name, 0),
                                        None => true, // fail closed
                                    };
                                    if mutates {
                                        out.insert(r);
                                    }
                                }
                            }
                        } else if let Some(existing) = st.closure_mutates.get(name.as_str()) {
                            // another local closure — union its mutation roots
                            out.extend(existing.iter().cloned());
                        } else {
                            // unknown callee — fail closed on ref roots
                            for a in cr.args {
                                let mut roots = Vec::new();
                                collect_ref_roots(&a.value, &mut roots);
                                for root in roots {
                                    if let Some(r) =
                                        resolve(&root, current_aliases, current_shadows)
                                    {
                                        out.insert(r);
                                    }
                                }
                            }
                        }
                    }
                    Expr::Field { base, .. } => {
                        if let Expr::Ident { name: pkg, .. } = &**base {
                            if A3_PURE_PKGS.contains(&pkg.as_str()) {
                                continue;
                            }
                        }
                        for a in cr.args {
                            let mut roots = Vec::new();
                            collect_ref_roots(&a.value, &mut roots);
                            for root in roots {
                                if let Some(r) =
                                    resolve(&root, current_aliases, current_shadows)
                                {
                                    out.insert(r);
                                }
                            }
                        }
                    }
                    _ => {
                        for a in cr.args {
                            let mut roots = Vec::new();
                            collect_ref_roots(&a.value, &mut roots);
                            for root in roots {
                                if let Some(r) =
                                    resolve(&root, current_aliases, current_shadows)
                                {
                                    out.insert(r);
                                }
                            }
                        }
                    }
                }
            }
        }
        for _ in 0..8 {
            let mut added = Vec::new();
            for r in &out {
                if let Some(targets) = match_aliases.get(r) {
                    for t in targets {
                        if !bound.contains(t) && !out.contains(t) {
                            added.push(t.clone());
                        }
                    }
                }
            }
            if added.is_empty() {
                break;
            }
            out.extend(added);
        }
        out
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
                self.check_addr_of_const_loan(st, val);
                if let Some(path) = walk::addr_of_index_base_path(val) {
                    let binding = name.clone();
                    let key = walk::addr_of_index_slot_key(val);
                    self.emit_o8_alias_if_live(
                        st,
                        &path,
                        key.as_deref(),
                        Some(&binding),
                        val.span(),
                        idx,
                        "a second slot loan",
                    );
                    let last = st.last_use.get(&binding).copied().unwrap_or(idx).max(idx);
                    st.loans.push(Loan {
                        target: LoanTarget::MapSlot {
                            binding: binding.clone(),
                            path,
                            key,
                        },
                        created: idx,
                        last,
                        span: val.span(),
                        desc: format!("`{} := {}`", binding, val.render()),
                    });
                } else {
                    self.extend_map_slot_loan_to_alias(st, name, val, idx);
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
        let is_let = matches!(l.kind, LocalKind::Let);
        // Issue #205: reject non-constant `const` initializers (emitter keeps Go `const`).
        if matches!(l.kind, LocalKind::Const) {
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                self.check_const_init_is_constant(name, value, Some(st));
            }
        }
        if l.values.is_empty() {
            // `var hits atomic.int64` / `var got []int` — shape from the
            // ascription (there is no initializer to infer from).
            if let Some(t) = &l.ty {
                self.check_zero_value_local(l, t);
            }
            let shape =
                l.ty.as_ref()
                    .map(|t| self.shape_of_type(t))
                    .unwrap_or(TypeShape::Unknown);
            let ty = l.ty.clone();
            for name in &l.names {
                self.define_binding(st, name, shape.clone(), ty.clone(), immutable, managed, is_let);
            }
            return;
        }
        if l.values.len() == 1 && l.names.len() > 1 {
            // multi-value define from one expression
            // Issue #146: `n int, ok = <-ch` / shared ascription on a receive.
            if let Some(t) = &l.ty {
                let want = self.shape_of_type(t);
                self.check_recv_elem_against_target(
                    st,
                    &l.values[0],
                    &want,
                    l.values[0].span(),
                    &format!("receive into `{}`", l.names[0]),
                );
            }
            let moved_non_owned = self.moved_non_owned_origin_in_expr(st, &l.values[0]);
            self.mark_move_in_value(
                st,
                &l.values[0],
                format!("the move-define `{}`", l.values[0].render()),
            );
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            let tys = self.multi_value_tys(st, &l.values[0], l.names.len());
            struct MultiDefInfo {
                name: String,
                shape: TypeShape,
                ty: Option<hy_syntax::TypeExpr>,
                reads_self: bool,
                shadowed_origin: Option<String>,
                prior_alias: Option<String>,
                prior_shares: Vec<String>,
            }
            let resolved: Vec<MultiDefInfo> = l
                .names
                .iter()
                .zip(shapes)
                .zip(tys)
                .map(|((name, shape), ty)| {
                    let (shape, ty) = match (&l.ty, shape) {
                        (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                        (_, s) => (s, ty.or_else(|| l.ty.clone())),
                    };
                    let reads_self = self.value_reads_storage_of(st, &l.values[0], name);
                    let shadowed_origin = if self.shape_may_share(&shape) && reads_self {
                        self.non_owned_origin_of(st, name).map(|(o, via_share)| {
                            if via_share && !o.starts_with("a may-share peer of ") {
                                format!("a may-share peer of {}", o)
                            } else {
                                o
                            }
                        })
                    } else {
                        None
                    };
                    let outer_exists = self.lookup_binding(st, name).is_some();
                    let prior_alias = if reads_self {
                        st.aliases.get(name).cloned().or_else(|| {
                            if self.shape_may_share(&shape)
                                && outer_exists
                                && !st.scopes.is_empty()
                            {
                                Some(name.clone())
                            } else {
                                None
                            }
                        })
                    } else {
                        None
                    };
                    let prior_shares: Vec<String> = if reads_self {
                        st.may_share
                            .get(name)
                            .map(|s| s.iter().cloned().collect())
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    MultiDefInfo {
                        name: name.clone(),
                        shape,
                        ty,
                        reads_self,
                        shadowed_origin,
                        prior_alias,
                        prior_shares,
                    }
                })
                .collect();
            let pre_st = st.clone();
            let mut overwritten = Vec::new();
            for info in resolved {
                overwritten.push(info.name.clone());
                let is_view = matches!(info.shape, TypeShape::BorrowHandle(_));
                let carries = self.shape_may_share(&info.shape);
                self.define_binding(st, &info.name, info.shape.clone(), info.ty, immutable, managed, is_let);
                if is_view {
                    self.extend_call_loans_to_view(st, &l.values[0], &info.name, idx);
                }
                if info.reads_self {
                    if let Some(pa) = info.prior_alias {
                        st.aliases.insert(info.name.clone(), pa);
                    }
                    for peer in info.prior_shares {
                        st.share_edge(&info.name, &peer);
                    }
                }
                self.record_storage_from_snapshot(st, &pre_st, &info.name, &l.values[0], &overwritten, true, true);
                if let Some(origin) = info.shadowed_origin {
                    if let Some(b) = st.env_lookup_mut(&info.name) {
                        if b.non_owned_origin.is_none() {
                            b.non_owned_origin = Some(origin);
                        }
                    }
                }
                if carries {
                    if let Some(origin) = &moved_non_owned {
                        if let Some(b) = st.env_lookup_mut(&info.name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin.clone());
                            }
                        }
                    }
                    self.record_move_const_and_a3_loans(st, &info.name, &l.values[0], idx);
                }
            }
            // Every name is bound now: join results the callee returns twice.
            self.link_multi_value_return_aliases(st, &l.values[0], &l.names);
        } else {
            let pre_st = st.clone();
            let mut overwritten = Vec::new();
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                overwritten.push(name.clone());
                // Issue #148: constant initializer outside a fixed-width
                // declared type → HYB-E6002 (Go would refuse to build).
                if let Some(t) = &l.ty {
                    self.check_const_overflow_init(name, t, value, Some(st));
                } else if l.kind != LocalKind::Const {
                    // Issue #228: untyped variable declarations use the
                    // target's default int type in emitted Go (the emitted
                    // Go declaration is `int` when no ascription is present).
                    self.check_default_int_overflow(name, value, Some(st));
                    self.check_default_float_overflow(name, value, Some(st));
                }
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
                } else if l.kind != LocalKind::Const
                    && Checker::is_nil_literal(value)
                {
                    // N1, untyped leg (HYB-E4003): `var x = nil` / `x := nil`
                    // / `let x = nil` — no ascription means nothing proves a
                    // nil home, `shape_of_expr(nil)` is Unknown, and the
                    // emitter emits `var x = nil`, which Go rejects outright
                    // (`use of untyped nil in variable declaration`). Local
                    // `const c = nil` is the E6003 constant-expression guard
                    // (issue #640) — different locus, do not double-fire.
                    self.emit_untyped_nil_decl_error(name, value.span());
                }
                // N1 for composite literals: `job{m: nil}` carries its own
                // type context (the struct def / map value / slice elem) —
                // the nested nil needs no ascription to reject (HYB-E4001).
                self.check_composite_nils(&pre_st, value);
                // Issue #146: typed receive `n int = <-ch` against `chan job`
                // used to accept and then fail `go build`. Compare the
                // ascription against the channel element when both resolve.
                if let Some(t) = &l.ty {
                    let want = self.shape_of_type(t);
                    self.check_recv_elem_against_target(
                        &pre_st,
                        value,
                        &want,
                        value.span(),
                        &format!("receive into `{}`", name),
                    );
                }
                // Ascription fills the gaps (O4.9 escape: "make the type
                // visible to the checker"): when the initializer's shape
                // does not resolve, the declared type decides. When inference
                // DOES resolve it keeps priority — it is more precise than
                // the ascription for boundary handles (`var v serde_json.view
                // = doc.array()` must stay a BorrowHandle for the loan
                // windows); a lying ascription is caught downstream by the
                // emitted Go build (the emitter carries the declared type).
                let inferred = self.shape_of_expr(&pre_st, value);
                let (shape, ty) = match (&l.ty, inferred) {
                    (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                    (_, inferred) => (
                        inferred,
                        l.ty.clone()
                            .or_else(|| self.ty_of_expr(&pre_st, value))
                            // Composite operands the type cache does not
                            // project (`sub := append(chs, ch)`): bind the
                            // projected type so later `<-sub[0]` receives
                            // resolve their direction (the "known remaining
                            // sibling" in recv_composite_channel_operands.rs).
                            .or_else(|| self.composite_type_expr(&pre_st, value)),
                    ),
                };
                let is_fresh_copy = self.call_returns_fresh_copy(&pre_st, value);
                let shadowed_const = !is_fresh_copy && self.shape_may_share(&shape)
                    && self.value_reads_storage_of(&pre_st, value, name)
                    && self
                        .const_write_root(
                            &pre_st,
                            &Place {
                                kind: PlaceKind::Ident(name.clone()),
                                span: value.span(),
                            },
                        )
                        .is_some();
                let immutable = immutable || shadowed_const;
                self.mark_move_in_value(
                    st,
                    value,
                    format!("the move-define `{}`", value.render()),
                );
                // `x := move y` — y is moved; x takes ownership (no alias);
                // non-ident moves (`x := move s.data`) trigger HYB-E3004 (O5.4).
                let moved_non_owned = self.moved_non_owned_origin_in_expr(&pre_st, value);
                if move_operand_ident(value).is_some()
                    || matches!(unwrap_parens(value), Expr::Move { .. })
                {
                    self.define_binding(st, name, shape, ty, immutable, managed, is_let);
                    if let Some(origin) = moved_non_owned {
                        if let Some(b) = st.env_lookup_mut(name) {
                            b.non_owned_origin = Some(origin);
                        }
                    }
                    self.record_move_const_and_a3_loans(st, name, value, idx);
                    continue;
                }
                // Self-shadowing define (`m := m`, `h := h`, `m := same(m)`
                // in an inner block): the RHS reads the OUTER binding, but
                // every link recorded below resolves `name` AFTER the define
                // — i.e. against the new inner binding itself — so the outer
                // binding's non-owned origin (param / receiver / capture /
                // range var, O5.1) is lost and `move m` passed HYB-E3004
                // while the caller still holds the same map. Read the origin
                // the outer binding would cite NOW, before it is shadowed.
                let reads_self = !is_fresh_copy && self.value_reads_storage_of(&pre_st, value, name);
                let shadowed_origin = if self.shape_may_share(&shape) && reads_self {
                    self.non_owned_origin_of(&pre_st, name).map(|(o, via_share)| {
                        if via_share && !o.starts_with("a may-share peer of ") {
                            format!("a may-share peer of {}", o)
                        } else {
                            o
                        }
                    })
                } else {
                    None
                };
                let outer_exists = pre_st.env.contains_key(name)
                    || pre_st.scopes.iter().any(|s| s.contains_key(name));
                let prior_alias = if reads_self {
                    pre_st.aliases.get(name).cloned().or_else(|| {
                        if self.shape_may_share(&shape) && outer_exists && !pre_st.scopes.is_empty()
                        {
                            Some(name.clone())
                        } else {
                            None
                        }
                    })
                } else {
                    None
                };
                let prior_children: Vec<String> = if reads_self {
                    pre_st
                        .aliases
                        .iter()
                        .filter_map(|(k, v)| if v == name && k != name { Some(k.clone()) } else { None })
                        .collect()
                } else {
                    Vec::new()
                };
                let prior_shares: Vec<String> = if reads_self {
                    pre_st
                        .may_share
                        .get(name)
                        .map(|s| s.iter().cloned().collect())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let prior_shadow_links = if reads_self {
                    pre_st.shadow_links.get(name).cloned().unwrap_or_default()
                } else {
                    HashSet::new()
                };
                let is_view = matches!(shape, TypeShape::BorrowHandle(_));
                let outer_depth = if pre_st
                    .scopes
                    .last()
                    .is_some_and(|top| !top.contains_key(name))
                {
                    pre_st.lookup_depth(name)
                } else {
                    None
                };
                self.define_binding(st, name, shape, ty, immutable, managed, is_let);
                if let Some(origin) = &moved_non_owned {
                    if let Some(b) = st.env_lookup_mut(name) {
                        if b.non_owned_origin.is_none() {
                            b.non_owned_origin = Some(origin.clone());
                        }
                    }
                }
                if reads_self {
                    if !prior_shadow_links.is_empty() {
                        st.shadow_links
                            .entry(name.clone())
                            .or_default()
                            .extend(prior_shadow_links);
                    }
                    if let Some(depth) = outer_depth {
                        st.shadow_links
                            .entry(name.clone())
                            .or_default()
                            .insert((name.clone(), depth));
                    }
                    if let Some(pa) = prior_alias {
                        st.aliases.insert(name.clone(), pa);
                    }
                    for child in prior_children {
                        if child != *name {
                            st.aliases.insert(child, name.clone());
                        }
                    }
                    for peer in prior_shares {
                        st.share_edge(name, &peer);
                    }
                }
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
                self.record_storage_from_snapshot(st, &pre_st, name, value, &overwritten, true, false);
                self.record_move_const_and_a3_loans(st, name, value, idx);
                // alias recording: `m2 := agg` (single-parent link, the
                // friendlier "frozen alias" diagnostic) — and call results
                // that hand back argument storage (#38).


                // #111 form 4: record the base paths a closure value's body
                // mutates, so a call inside a live slot-loan window counts
                // as an A3 insert/delete event.
                self.record_closure_mutations(st, name, value);
                // #141 (pr-140): the self-shadowing define keeps the SHADOWED
                // binding's non-owned origin — the RHS read the outer
                // storage, so the inner binding inherits O5.1's restriction
                // (move-at-boundary still rejects E3004).
                if let Some(origin) = shadowed_origin.or(moved_non_owned) {
                    if let Some(b) = st.env_lookup_mut(name) {
                        if b.non_owned_origin.is_none() {
                            b.non_owned_origin = Some(origin);
                        }
                    }
                }
            }
        }
    }

    /// Does `value` hand back (part of) the storage of the binding `name`,
    /// in one of the forms `record_may_share` / `record_alias` link: a bare
    /// or parenthesized copy, a re-slice, a reference-bearing projection, a
    /// composite literal embedding it, or a call that takes it as a
    /// non-`move` argument / receiver and returns a value that may share.
    /// Used for self-shadowing defines and self-rebind assignments, where
    /// those recorders cannot tell the prior `name` from the new one.
    pub(crate) fn value_reads_storage_of(&self, st: &FnState, value: &Expr, name: &str) -> bool {
        let root_is = |e: &Expr| e.root_ident() == Some(name);
        match unwrap_parens(value) {
            Expr::Ident { name: src, .. } => src == name,
            Expr::Try { operand, .. } => self.value_reads_storage_of(st, operand, name),
            Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                ..
            } => root_is(operand) || self.value_reads_storage_of(st, operand, name),
            Expr::Slice { base, .. } => {
                root_is(unwrap_parens(base)) || self.value_reads_storage_of(st, base, name)
            }
            Expr::Field { base, .. } | Expr::Index { base, .. } => {
                let sh = self.shape_of_expr(st, unwrap_parens(value));
                (self.shape_may_share(&sh) || matches!(sh, TypeShape::Unknown))
                    && (root_is(value) || self.value_reads_storage_of(st, base, name))
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| self.value_reads_storage_of(st, &f.value, name)),
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                self.value_reads_storage_of(st, k, name) || self.value_reads_storage_of(st, v, name)
            }),
            Expr::SliceLit { items, .. } => items
                .iter()
                .any(|it| self.value_reads_storage_of(st, it, name)),
            Expr::Call { callee, args, .. } => {
                if self.call_returns_fresh_copy(st, value) {
                    return false;
                }
                let result = self.shape_of_expr(st, unwrap_parens(value));
                // A later result can share storage even when result 0 is primitive.
                let shares_storage = self.shape_may_share(&result)
                    || matches!(result, TypeShape::Unknown)
                    || self.call_result_tys(st, callee).is_some_and(|results| {
                        results.iter().any(|ty| self.shape_may_share(&self.shape_of_type(ty)))
                    });
                if !shares_storage {
                    return false;
                }
                let via_arg = args.iter().any(|a| {
                    !matches!(unwrap_parens(&a.value), Expr::Move { .. })
                        && (root_is(&a.value) || self.value_reads_storage_of(st, &a.value, name))
                });
                let via_recv = match unwrap_parens(callee) {
                    Expr::Field { base, .. } => {
                        root_is(base) || self.value_reads_storage_of(st, base, name)
                    }
                    _ => false,
                };
                if via_arg || via_recv {
                    return true;
                }
                let mut const_returns = Vec::new();
                self.collect_callee_const_return_roots(st, value, &mut const_returns, 0);
                const_returns.iter().any(|c| c == name)
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut bound: std::collections::HashSet<String> =
                    params.iter().map(|p| p.name.clone()).collect();
                bound.extend(results.iter().filter_map(|r| r.name.clone()));
                if bound.contains(name) {
                    false
                } else {
                    let mut idents = Vec::new();
                    collect_capture_idents_block(body, &mut bound, &mut idents);
                    idents.iter().any(|id| id == name)
                }
            }
            _ => false,
        }
    }

    /// Collect root identifiers that are borrowed via address-of (`&x`) inside `e`,
    /// descending through composite literals, re-slices, and borrow-preserving
    /// projections or calls (`[]*node{&n}[:]`, `[]*node{&n}[0]`, `append([]*node{}, &n)`),
    /// while stopping at scalar-returning calls (`count_items(&n)` returning `int`).
    pub(crate) fn collect_expr_addr_of_roots(&self, st: &FnState, e: &Expr, out: &mut Vec<String>) {
        match unwrap_parens(e) {
            Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                ..
            } => {
                if let Some(id) = operand.root_ident() {
                    out.push(id.to_string());
                }
                self.collect_expr_addr_of_roots(st, operand, out);
            }
            Expr::Try { operand, .. } | Expr::Move { operand, .. } => {
                self.collect_expr_addr_of_roots(st, operand, out);
            }
            Expr::Slice { base, .. } => {
                self.collect_expr_addr_of_roots(st, base, out);
            }
            Expr::Field { base, .. } | Expr::Index { base, .. } => {
                let sh = self.shape_of_expr(st, e);
                if field_borrow_kind(&sh).is_some() || matches!(sh, TypeShape::Unknown) {
                    self.collect_expr_addr_of_roots(st, base, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                if self.expr_has_borrow_shape(st, e) {
                    if let Expr::Field { base, .. } = unwrap_parens(callee) {
                        self.collect_expr_addr_of_roots(st, base, out);
                    }
                    for a in args {
                        self.collect_expr_addr_of_roots(st, &a.value, out);
                    }
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.collect_expr_addr_of_roots(st, &f.value, out);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.collect_expr_addr_of_roots(st, k, out);
                    self.collect_expr_addr_of_roots(st, v, out);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.collect_expr_addr_of_roots(st, it, out);
                }
            }
            _ => {}
        }
    }

    /// A tuple call can carry a borrow in any result, not just the first.
    fn expr_has_borrow_shape(&self, st: &FnState, e: &Expr) -> bool {
        let inner = unwrap_parens(e);
        if let Expr::Call { callee, .. } = inner {
            if let Some(results) = self.call_result_tys(st, callee) {
                return results.iter().any(|ty| {
                    let shape = self.shape_of_type(ty);
                    field_borrow_kind(&shape).is_some() || matches!(shape, TypeShape::Unknown)
                });
            }
        }
        let shape = self.shape_of_expr(st, inner);
        field_borrow_kind(&shape).is_some() || matches!(shape, TypeShape::Unknown)
    }

    pub(crate) fn expr_carries_borrow(&self, st: &FnState, e: &Expr) -> bool {
        let inner = unwrap_parens(e);
        if as_addr_of(inner).is_some() || contains_addr_of_in_literal(inner) {
            return true;
        }
        if !self.expr_has_borrow_shape(st, inner) {
            return false;
        }
        let mut addr_roots = Vec::new();
        self.collect_expr_addr_of_roots(st, inner, &mut addr_roots);
        if !addr_roots.is_empty() {
            return true;
        }
        let mut ref_roots = Vec::new();
        collect_ref_roots(inner, &mut ref_roots);
        ref_roots.iter().any(|r| {
            st.borrow_bindings.contains(r)
                || st
                    .loans
                    .iter()
                    .any(|l| matches!(&l.target, LoanTarget::MapSlot { binding, .. } if binding == r))
        })
    }


    /// Does the binding `name` denote a value that a shallow Go copy
    /// would share mutable GC storage with? Includes file-level `const`
    /// composites (#125), which lower to package-level `var` in Go.
    fn binding_may_share(&self, st: &FnState, name: &str) -> bool {
        self.lookup_binding(st, name)
            .map(|b| self.shape_may_share(&b.shape))
            .unwrap_or(false)
    }

    /// Does `name` denote a mutable composite (including a scalar-field struct
    /// or primitive-element array whose `TypeShape` is `Primitive`) that shares
    /// backing storage when borrowed via `&x` or auto-addressed into a `*T`
    /// parameter/receiver (#131)?
    fn binding_can_be_borrowed(&self, st: &FnState, name: &str) -> bool {
        let Some(b) = self.lookup_binding(st, name) else {
            return false;
        };
        if self.shape_may_share(&b.shape)
            || matches!(b.shape, TypeShape::Struct(_) | TypeShape::Array(_))
        {
            return true;
        }
        if let Some(ty) = &b.ty {
            let resolved = self.resolve_type_alias(ty);
            match &resolved {
                hy_syntax::TypeExpr::Array { .. } => return true,
                hy_syntax::TypeExpr::Named { name: tname, .. } => {
                    let base = Self::base_named_type_str(tname);
                    if self.structs.contains_key(base) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn call_param_is_pointer(&self, st: &FnState, callee: &Expr, idx: usize) -> bool {
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(hy_syntax::TypeExpr::Fn { params, .. }) = &b.ty {
                        return params
                            .get(idx)
                            .is_some_and(|p| matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. }));
                    }
                    return false;
                }
                let head = Self::base_named_type_str(name);
                self.fns
                    .get(name.as_str())
                    .or_else(|| self.fns.get(head))
                    .and_then(|f| f.params.get(idx))
                    .is_some_and(|p| matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. }))
            }
            Expr::Field { base, name, .. } => {
                let head = Self::base_named_type_str(name);
                if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, root).is_none() {
                        let full = format!("{}.{}", root, head);
                        if let Some(f) = self.fns.get(full.as_str()) {
                            return f
                                .params
                                .get(idx)
                                .is_some_and(|p| matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. }));
                        }
                    }
                }
                let recv_base = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match self.shape_of_expr(st, base) {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(&s).to_string())
                        }
                        TypeShape::Pointer(inner) => match *inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(&s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                recv_base
                    .and_then(|rb| self.method_decls.get(&(rb, head.to_string())))
                    .and_then(|f| f.params.get(idx))
                    .is_some_and(|p| matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. }))
            }
            Expr::FnLit { params, .. } => params
                .get(idx)
                .is_some_and(|p| matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. })),
            _ => false,
        }
    }

    fn call_receiver_is_pointer(&self, st: &FnState, callee: &Expr) -> bool {
        let Expr::Field { base, name, .. } = unwrap_parens(callee) else {
            return false;
        };
        let head = Self::base_named_type_str(name);
        let recv_base = self
            .ty_of_expr(st, base)
            .map(|t| self.resolve_type_alias(&t))
            .and_then(|t| Self::base_named_type(&t))
            .or_else(|| match self.shape_of_expr(st, base) {
                TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(&s).to_string()),
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(&s).to_string()),
                    _ => None,
                },
                _ => None,
            });
        recv_base
            .and_then(|rb| self.method_decls.get(&(rb, head.to_string())))
            .and_then(|f| f.receiver.as_ref())
            .is_some_and(|r| r.is_pointer || matches!(r.ty, hy_syntax::TypeExpr::Pointer { .. }))
    }

    /// A projection of a call-produced value has independent storage only
    /// when the call returns a value copy or a proven fresh slice backing array.
    fn addr_operand_returns_fresh_copy(&self, st: &FnState, expr: &Expr) -> bool {
        match unwrap_parens(expr) {
            Expr::Index { base, .. } | Expr::Field { base, .. } | Expr::Slice { base, .. } => {
                self.addr_operand_returns_fresh_copy(st, base)
            }
            Expr::Call { callee, .. } => {
                let shape = self.shape_of_expr(st, expr);
                (!matches!(shape, TypeShape::Unknown) && !self.shape_may_share(&shape))
                    || (matches!(unwrap_parens(callee), Expr::Ident { name, .. }
                        if self.lookup_binding(st, name).is_none())
                        && self.call_returns_fresh_copy(st, expr))
            }
            _ => false,
        }
    }

    /// Collect root identifiers borrowed in `e` either via explicit address-of
    /// (`&x`, `&m[k]`, `&s.f`) or via `hy-emit` auto-addressing of a value
    /// argument/receiver into a `*T` parameter/receiver (`id_ptr(s)`, `s.ptr()`)
    /// when the enclosing call/projection can return shared storage (#131).
    fn collect_borrowed_roots(&self, st: &FnState, e: &Expr, out: &mut Vec<String>) {
        collect_addr_of_roots(e, out);
        let inner = unwrap_parens(e);
        match inner {
            Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                ..
            } => {
                if operand.root_ident().is_none()
                    && matches!(unwrap_parens(operand), Expr::Index { .. } | Expr::Field { .. })
                    && !self.addr_operand_returns_fresh_copy(st, operand)
                {
                    collect_ref_roots(operand, out);
                    self.collect_callee_const_return_roots(st, operand, out, 0);
                }
                self.collect_borrowed_roots(st, operand, out);
            }
            Expr::Try { operand, .. } | Expr::Move { operand, .. } => {
                self.collect_borrowed_roots(st, operand, out);
            }
            Expr::Slice { base, .. }
            | Expr::Field { base, .. }
            | Expr::Index { base, .. } => {
                let proj_shape = self.shape_of_expr(st, inner);
                if self.shape_may_share(&proj_shape) || matches!(proj_shape, TypeShape::Unknown) {
                    self.collect_borrowed_roots(st, base, out);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.collect_borrowed_roots(st, &f.value, out);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.collect_borrowed_roots(st, k, out);
                    self.collect_borrowed_roots(st, v, out);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.collect_borrowed_roots(st, it, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                let call_res = self.shape_of_expr(st, inner);
                if self.shape_may_share(&call_res) || self.expr_has_borrow_shape(st, inner) {
                    for (idx, a) in args.iter().enumerate() {
                        if !matches!(unwrap_parens(&a.value), Expr::Move { .. }) {
                            let arg_shape = self.shape_of_expr(st, &a.value);
                            if !matches!(arg_shape, TypeShape::Pointer(_))
                                && self.call_param_is_pointer(st, callee, idx)
                            {
                                if let Some(root) = a.value.root_ident() {
                                    if self.binding_can_be_borrowed(st, root) {
                                        out.push(root.to_string());
                                    }
                                }
                            }
                            self.collect_borrowed_roots(st, &a.value, out);
                        }
                    }
                    if let Expr::Field { base, .. } = unwrap_parens(callee) {
                        let recv_shape = self.shape_of_expr(st, base);
                        if !matches!(recv_shape, TypeShape::Pointer(_))
                            && self.call_receiver_is_pointer(st, callee)
                        {
                            if let Some(root) = base.root_ident() {
                                if self.binding_can_be_borrowed(st, root) {
                                    out.push(root.to_string());
                                }
                            }
                        }
                        self.collect_borrowed_roots(st, base, out);
                    }
                }
            }
            _ => {}
        }
    }

    /// Record may-share edges for a define/rebind `name <op> value` (#27).
    /// Covers the four reported bypass forms: `h2 := h1` (shallow struct/
    /// array copy), `m2 := (agg)` / `s2 := s1[:]` (parenthesized / re-sliced
    /// RHS), `m2 := h1.m` (field/index projection), and `h := holder{m: agg}`
    /// (composite literal capturing a live binding) — plus the CALL forms of
    /// #73: a shallow copy or generic identity that returns argument
    /// storage through `f(...)` (no interprocedural summary, same argument
    /// as #38's alias recording).
    /// Evaluate storage sources before any parallel LHS changes its binding.
    /// Sources already overwritten by an earlier LHS refer to their surviving
    /// peers (or the saved outer scope), never to the replacement value.
    pub(crate) fn record_storage_from_snapshot(
        &self,
        st: &mut FnState,
        pre_st: &FnState,
        name: &str,
        value: &Expr,
        overwritten: &[String],
        local: bool,
        multi: bool,
    ) {
        if name == "_" || self.call_returns_fresh_copy(pre_st, value) {
            return;
        }
        // Preserve the ordinary alias path (including non-owned origins)
        // unless an earlier LHS actually replaced storage read by this RHS.
        let reads_overwritten = move_operand_ident(value).is_none()
            && overwritten.iter().any(|prev| {
                prev != name && prev != "_" && self.value_reads_storage_of(pre_st, value, prev)
            });
        if !reads_overwritten {
            if multi {
                let shape = self.lookup_binding(st, name)
                    .map(|b| b.shape.clone()).unwrap_or(TypeShape::Unknown);
                self.record_multi_value_share_and_alias(st, name, &shape, value);
            } else {
                self.record_may_share(st, name, value);
                self.record_alias(st, name, value);
            }
            return;
        }
        let mut rhs = pre_st.clone();
        rhs.aliases.remove(name);
        rhs.share_clear(name);
        if let Some(binding) = rhs.env_lookup_mut(name) {
            binding.non_owned_origin = None;
        }
        if multi {
            let shape = self.lookup_binding(st, name).map(|b| b.shape.clone()).unwrap_or(TypeShape::Unknown);
            self.record_multi_value_share_and_alias(&mut rhs, name, &shape, value);
        } else {
            self.record_may_share(&mut rhs, name, value);
            self.record_alias(&mut rhs, name, value);
        }
        if let Some(origin) = self.lookup_binding(&rhs, name).and_then(|b| b.non_owned_origin.clone()) {
            if let Some(binding) = st.env_lookup_mut(name) {
                if binding.non_owned_origin.is_none() {
                    binding.non_owned_origin = Some(origin);
                }
            }
        }
        let alias = rhs.aliases.get(name).cloned();
        let mut sources = rhs.may_share.get(name).cloned().unwrap_or_default();
        if let Some(source) = &alias {
            sources.insert(source.clone());
        }
        for source in sources {
            let mut peers = self.storage_peers(pre_st, &source);
            peers.insert(0, source.clone());
            for peer in peers {
                if overwritten.contains(&peer) {
                    if local && pre_st.scopes.last().is_some_and(|top| !top.contains_key(&peer)) {
                        if let Some(depth) = pre_st.lookup_depth(&peer) {
                            st.shadow_links.entry(name.to_string()).or_default().insert((peer.clone(), depth));
                        }
                    }
                } else if peer != name {
                    st.share_edge(name, &peer);
                    if alias.as_ref() == Some(&source) && !st.aliases.contains_key(name) {
                        st.aliases.insert(name.to_string(), peer.clone());
                    }
                }
                if let Some(links) = pre_st.shadow_links.get(&peer) {
                    st.shadow_links.entry(name.to_string()).or_default().extend(links.iter().cloned());
                }
            }
        }
    }

    pub(crate) fn record_may_share(&self, st: &mut FnState, name: &str, value: &Expr) {
        if name == "_" {
            return;
        }
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
            // #109 (pr-110): a closure value holds live references to every
            // outer binding its body captures — invoking it after a `move`
            // of a captured binding is the alias-read/alias-write form of
            // the freeze rule. Link the closure binding to each captured
            // reference-bearing outer binding so a later `move` of either
            // freezes the other (O5.2), exactly like a shallow copy.
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut bound: std::collections::HashSet<String> =
                    params.iter().map(|p| p.name.clone()).collect();
                bound.extend(results.iter().filter_map(|r| r.name.clone()));
                let mut idents = Vec::new();
                collect_capture_idents_block(body, &mut bound, &mut idents);
                for id in idents {
                    if self.binding_may_share(st, &id)
                        || self
                            .lookup_binding(st, &id)
                            .map(|b| !b.immutable && matches!(b.shape, TypeShape::Array(_)))
                            .unwrap_or(false)
                    {
                        st.share_edge(name, &id);
                    }
                }
            }
            _ => {
                if self.call_returns_fresh_copy(st, value) {
                    return;
                }
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
                self.collect_callee_const_return_roots(st, value, &mut roots, 0);
                for root in roots {
                    if root == name {
                        continue;
                    }
                    if let Some(b) = self.lookup_binding(st, &root) {
                        if b.shape.is_handle()
                            && !self.shape_carries_handle(&result)
                            && !matches!(result, TypeShape::Unknown)
                        {
                            // O9: boundary calls on a handle that return a pure
                            // Go value (e.g. `out := enc.finish()?` returning
                            // `[]byte`) produce a fresh Go copy, not an alias
                            // of the handle.
                            continue;
                        }
                        // pr-319 (#318): a slice of an ADDRESSABLE ARRAY
                        // views the array's storage even when the element
                        // type is primitive - link it so a later move
                        // freezes the array too.
                        if self.shape_may_share(&b.shape)
                            || self.expr_slices_addressable_array(st, value, &root)
                        {
                            st.share_edge(name, &root);
                        }
                    }
                }
                if let Some(origin) = self.moved_non_owned_origin_in_expr(st, value) {
                    if let Some(nb) = st.env_lookup_mut(name) {
                        if nb.non_owned_origin.is_none() {
                            nb.non_owned_origin = Some(origin);
                        }
                    }
                }
                let mut addr_roots = Vec::new();
                self.collect_borrowed_roots(st, value, &mut addr_roots);
                for root in addr_roots {
                    if root != name {
                        st.share_edge(name, &root);
                        let origin = self
                            .lookup_binding(st, &root)
                            .and_then(|b| b.non_owned_origin.clone());
                        if let Some(origin) = origin {
                            if let Some(nb) = st.env_lookup_mut(name) {
                                if nb.non_owned_origin.is_none() {
                                    nb.non_owned_origin = Some(origin);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    /// True when `value` contains a slice whose base is an addressable
    /// array rooted at `root`. A call that returns an array by value
    /// (`clone(a)[:]`) is a copy: the slice base is the call, so it does
    /// not match even though `collect_ref_roots` still sees `a`.
    fn expr_slices_addressable_array(&self, st: &FnState, value: &Expr, root: &str) -> bool {
        match value {
            Expr::Slice { base, lo, hi, .. } => {
                if self.addressable_array_root(st, base) == Some(root) {
                    return true;
                }
                if self.expr_slices_addressable_array(st, base, root) {
                    return true;
                }
                if let Some(lo) = lo {
                    if self.expr_slices_addressable_array(st, lo, root) {
                        return true;
                    }
                }
                if let Some(hi) = hi {
                    if self.expr_slices_addressable_array(st, hi, root) {
                        return true;
                    }
                }
                false
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => {
                self.expr_slices_addressable_array(st, inner, root)
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_slices_addressable_array(st, lhs, root)
                    || self.expr_slices_addressable_array(st, rhs, root)
            }
            Expr::Index { base, index, .. } => {
                self.expr_slices_addressable_array(st, base, root)
                    || self.expr_slices_addressable_array(st, index, root)
            }
            Expr::Field { base, .. } => self.expr_slices_addressable_array(st, base, root),
            Expr::Call { callee, args, .. } => {
                self.expr_slices_addressable_array(st, callee, root)
                    || args
                        .iter()
                        .any(|a| self.expr_slices_addressable_array(st, &a.value, root))
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| self.expr_slices_addressable_array(st, &f.value, root)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .any(|it| self.expr_slices_addressable_array(st, it, root)),
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                self.expr_slices_addressable_array(st, k, root)
                    || self.expr_slices_addressable_array(st, v, root)
            }),
            Expr::Receive { chan, .. } => self.expr_slices_addressable_array(st, chan, root),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                hy_syntax::InterpPart::Expr(e) => {
                    self.expr_slices_addressable_array(st, e, root)
                }
                hy_syntax::InterpPart::Lit(_) => false,
            }),
            // Closure bodies are checked as their own statements.
            Expr::FnLit { .. } | Expr::Ident { .. } | Expr::Lit { .. } => false,
        }
    }

    /// Binding an addressable `[N]T` expression is rooted at.
    /// `a`, `b.data`, and `g[i]` qualify; a call does not (it yields a copy).
    fn addressable_array_root<'e>(&self, st: &FnState, expr: &'e Expr) -> Option<&'e str> {
        let expr = unwrap_parens(expr);
        if !matches!(self.shape_of_expr(st, expr), TypeShape::Array(_)) {
            return None;
        }
        expr.root_ident()
    }
    // `record_literal_ref` (pr-28's composite-literal recursion) is gone:
    // `record_may_share` now walks literals, calls and views with
    // `collect_ref_roots`, so a literal field that embeds a live binding —
    // bare (`holder{m: agg}`), projected (`holder{m: h1.m}`) or produced by
    // a call (`wrap{items: same(agg)}`) — is covered by one collector.

    /// Preserve T22 `const` root linkage and live A3 map-slot loans across
    /// `move` expressions (`c := move K`, `c = move K`, `h := holder{s: move K}`,
    /// `m2 := move m` during a live `&m[k]` loan). Linking `name` only to a
    /// resolved `const` root or propagating live `MapSlot` loans avoids
    /// adding `may_share` edges on ordinary mutable bindings, which would
    /// reclassify `use-after-move` on the source binding as `frozen alias`.
    pub(crate) fn record_move_const_and_a3_loans(
        &self,
        st: &mut FnState,
        name: &str,
        value: &Expr,
        idx: usize,
    ) {
        let mut moved_srcs = Vec::new();
        collect_move_ident_sources(value, &mut moved_srcs);
        if moved_srcs.is_empty() {
            return;
        }
        for src in moved_srcs {
            if src == name {
                continue;
            }
            let place = Place {
                kind: PlaceKind::Ident(src.clone()),
                span: value.span(),
            };
            if let Some(const_root) = self.const_write_root(st, &place) {
                st.share_edge(name, &const_root);
            }
            let inherited: Vec<Loan> = st
                .loans
                .iter()
                .filter_map(|l| match &l.target {
                    LoanTarget::MapSlot { binding, path, key }
                        if l.created <= idx
                            && idx <= l.last
                            && a3_paths_conflict(st, path, &src) =>
                    {
                        Some(Loan {
                            target: LoanTarget::MapSlot {
                                binding: binding.clone(),
                                path: name.to_string(),
                                key: key.clone(),
                            },
                            created: l.created,
                            last: l.last,
                            span: l.span,
                            desc: l.desc.clone(),
                        })
                    }
                    _ => None,
                })
                .collect();
            st.loans.extend(inherited);
        }
    }
    /// Record the may-share edge created by the builtin `copy(dst, src)`
    /// (#178). For a slice whose ELEMENT carries mutable GC storage
    /// (map / slice / pointer, or a struct/array of them) `copy` is
    /// element-wise: it duplicates the element *references*, so `dst[i]`
    /// and `src[i]` alias the same object, and a later `move` of either
    /// binding must freeze the other (O5.2). A primitive-element slice
    /// (`[]int`) copies by value and shares nothing, so no edge is recorded
    /// and the positive control stays accepted.
    pub(crate) fn record_copy_may_share(&self, st: &mut FnState, cr: &CallRef) {
        let (Some(dst), Some(src)) = (cr.args.first(), cr.args.get(1)) else {
            return;
        };
        // A slice shape is always a reference (`shape_may_share(Slice)` is
        // unconditional), so consult the ELEMENT shape instead: only a
        // reference-bearing element is copied by reference. Check both `dst`
        // and `src` so an untyped/unknown slice on one side still records the
        // edge when the other side's element type is known to share.
        let elem_shares = [&dst.value, &src.value].into_iter().any(|arg| {
            matches!(
                self.shape_of_expr(st, unwrap_parens(arg)),
                TypeShape::Slice(elem) if self.shape_may_share(&elem)
            )
        });
        if !elem_shares {
            return;
        }
        let mut dst_roots = Vec::new();
        collect_ref_roots(&dst.value, &mut dst_roots);
        let mut src_roots = Vec::new();
        collect_ref_roots(&src.value, &mut src_roots);
        for d in &dst_roots {
            for s in &src_roots {
                if d != s && self.binding_may_share(st, d) && (self.binding_may_share(st, s) || self.expr_slices_addressable_array(st, &src.value, s)) {
                    st.share_edge(d, s);
                }
            }
        }
        if let Some(origin) = self.moved_non_owned_origin_in_expr(st, &src.value) {
            let peer_origin = if origin.starts_with("a may-share peer of ") {
                origin
            } else {
                format!("a may-share peer of {}", origin)
            };
            for d in &dst_roots {
                if let Some(db) = st.env_lookup_mut(d) {
                    if db.non_owned_origin.is_none() {
                        db.non_owned_origin = Some(peer_origin.clone());
                    }
                }
            }
        }
    }

    /// Issue #455: interprocedural out-parameter / receiver store tracking.
    ///
    /// When a helper function, user-defined struct method, or inline closure
    /// stores one reference-bearing parameter/receiver (or a value derived from
    /// it) into an index or field rooted at another parameter/receiver (or
    /// copies elements between them via `copy(dst, src)` or a transitive
    /// helper call), the caller's destination and source bindings share mutable
    /// GC storage after the call returns. Record `st.share_edge(dst, src)` and
    /// propagate any `non_owned_origin` so a subsequent `move` of either caller
    /// binding freezes the other (O5.2, `HYB-E3003`) and cannot launder a
    /// non-owned parameter/capture/const across a concurrency boundary
    /// (O5.1 / O5.4, `HYB-E3004`; T22, `HYB-E3006`).
    pub(crate) fn record_callee_param_stores(&self, st: &mut FnState, cr: &CallRef) {
        let Some((slots, body)) = self.resolve_callee_slots_and_body(st, cr) else {
            return;
        };
        if slots.len() < 2 {
            return;
        }
        let slots_meta: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> = slots
            .iter()
            .map(|(name, shape, ty, _)| (name.as_str(), shape.clone(), ty.as_ref()))
            .collect();
        let pairs = self.collect_callee_param_stores(&slots_meta, &body, 0);
        for (dst_idx, src_idx) in pairs {
            if dst_idx == src_idx {
                continue;
            }
            let (Some((_, _, _, dst_src)), Some((_, _, _, src_src))) =
                (slots.get(dst_idx), slots.get(src_idx))
            else {
                continue;
            };
            let mut dst_roots = Vec::new();
            match dst_src {
                CalleeSlotSource::Arg(dst_expr) => {
                    collect_ref_roots(dst_expr, &mut dst_roots);
                    collect_addr_of_roots(dst_expr, &mut dst_roots);
                }
                CalleeSlotSource::OuterBinding(name) => {
                    dst_roots.push(name.clone());
                }
            }

            let mut src_roots = Vec::new();
            let src_origin = match src_src {
                CalleeSlotSource::Arg(src_expr) => {
                    collect_ref_roots(src_expr, &mut src_roots);
                    collect_addr_of_roots(src_expr, &mut src_roots);
                    let mut move_ops = Vec::new();
                    crate::model::collect_move_operands(src_expr, &mut move_ops);
                    for (op, _) in move_ops {
                        if let Expr::Ident { name, .. } = op {
                            src_roots.push(name.clone());
                        } else if let Some(r) = op.root_ident() {
                            src_roots.push(r.to_string());
                        }
                    }
                    self.moved_non_owned_origin_in_expr(st, src_expr)
                }
                CalleeSlotSource::OuterBinding(name) => {
                    src_roots.push(name.clone());
                    self.non_owned_origin_of(st, name)
                        .map(|(o, _)| o)
                        .or_else(|| {
                            self.lookup_binding(st, name)
                                .and_then(|b| b.non_owned_origin.clone())
                        })
                }
            };

            for d in &dst_roots {
                if !self.binding_may_share(st, d) {
                    continue;
                }
                for s in &src_roots {
                    if d != s
                        && (self.binding_may_share(st, s)
                            || self.lookup_binding(st, s).is_some_and(|b| b.immutable) || matches!(src_src, CalleeSlotSource::Arg(e) if self.expr_slices_addressable_array(st, e, s)))
                    {
                        st.share_edge(d, s);
                    }
                }
            }
            if let Some(origin) = src_origin {
                let peer_origin = if origin.starts_with("a may-share peer of ") {
                    origin
                } else {
                    format!("a may-share peer of {}", origin)
                };
                for d in &dst_roots {
                    if self.binding_may_share(st, d) {
                        if let Some(db) = st.env_lookup_mut(d) {
                            if db.non_owned_origin.is_none() {
                                db.non_owned_origin = Some(peer_origin.clone());
                            }
                        }
                    }
                }
            }
        }
    }

    fn append_outer_binding_slots<'c>(
        &self,
        st: &FnState,
        bound: &mut HashSet<String>,
        include_locals: bool,
        slots: &mut Vec<(
            String,
            TypeShape,
            Option<hy_syntax::TypeExpr>,
            CalleeSlotSource<'c>,
        )>,
    ) {
        if include_locals {
            for scope in st.scopes.iter().rev() {
                for (bname, b) in scope {
                    if bound.insert(bname.clone())
                        && (self.shape_may_share(&b.shape)
                            || matches!(b.shape, TypeShape::Unknown)
                            || b.immutable)
                    {
                        slots.push((
                            bname.clone(),
                            b.shape.clone(),
                            b.ty.clone(),
                            CalleeSlotSource::OuterBinding(bname.clone()),
                        ));
                    }
                }
            }
            for (bname, b) in &st.env {
                if bound.insert(bname.clone())
                    && (self.shape_may_share(&b.shape)
                        || matches!(b.shape, TypeShape::Unknown)
                        || b.immutable)
                {
                    slots.push((
                        bname.clone(),
                        b.shape.clone(),
                        b.ty.clone(),
                        CalleeSlotSource::OuterBinding(bname.clone()),
                    ));
                }
            }
        }
        for (cname, cb) in &self.consts {
            if bound.insert(cname.clone())
                && (self.shape_may_share(&cb.shape) || matches!(cb.shape, TypeShape::Unknown))
            {
                slots.push((
                    cname.clone(),
                    cb.shape.clone(),
                    cb.ty.clone(),
                    CalleeSlotSource::OuterBinding(cname.clone()),
                ));
            }
        }
    }

    fn resolve_callee_slots_and_body<'c>(
        &self,
        st: &FnState,
        cr: &'c CallRef<'c>,
    ) -> Option<(
        Vec<(
            String,
            TypeShape,
            Option<hy_syntax::TypeExpr>,
            CalleeSlotSource<'c>,
        )>,
        std::borrow::Cow<'a, Block>,
    )> {
        match crate::model::unwrap_parens(cr.callee) {
            Expr::Ident { name, .. } => {
                if name == "copy"
                    || name == "delete"
                    || name == "clear"
                    || name == "make"
                    || name == "append"
                    || name == "some"
                    || name == "new"
                    || name == "close"
                    || A3_PURE_FNS.contains(&name.as_str())
                {
                    return None;
                }
                if self.lookup_binding(st, name).is_some() {
                    let (params, results, body) = st.closure_defs.get(name.as_str())?;
                    let mut slots = Vec::new();
                    let mut bound: HashSet<String> =
                        params.iter().map(|p| p.name.clone()).collect();
                    bound.extend(results.iter().filter_map(|r| r.name.clone()));
                    for (i, p) in params.iter().enumerate() {
                        if let Some(arg) = cr.args.get(i) {
                            slots.push((
                                p.name.clone(),
                                self.shape_of_type(&p.ty),
                                Some(p.ty.clone()),
                                CalleeSlotSource::Arg(&arg.value),
                            ));
                        }
                    }
                    self.append_outer_binding_slots(st, &mut bound, true, &mut slots);
                    return Some((slots, std::borrow::Cow::Owned(body.clone())));
                }
                let decl = *self.fns.get(name.as_str())?;
                let body = decl.body.as_ref()?;
                let mut slots = Vec::new();
                let mut bound: HashSet<String> =
                    decl.params.iter().map(|p| p.name.clone()).collect();
                bound.extend(decl.results.iter().filter_map(|r| r.name.clone()));
                for (i, p) in decl.params.iter().enumerate() {
                    if let Some(arg) = cr.args.get(i) {
                        slots.push((
                            p.name.clone(),
                            self.shape_of_type(&p.ty),
                            Some(p.ty.clone()),
                            CalleeSlotSource::Arg(&arg.value),
                        ));
                    }
                }
                self.append_outer_binding_slots(st, &mut bound, false, &mut slots);
                Some((slots, std::borrow::Cow::Borrowed(body)))
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                if let Expr::Ident { name: pkg, .. } = crate::model::unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() && self.known_package(pkg) {
                        return None;
                    }
                }
                let recv_struct = match self.shape_of_expr(st, base) {
                    TypeShape::Struct(Some(s)) => {
                        Some(Self::base_named_type_str(&s).to_string())
                    }
                    TypeShape::Pointer(inner) => match *inner {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(&s).to_string())
                        }
                        _ => None,
                    },
                    _ => None,
                };
                let decl = if let Some(s) = recv_struct {
                    self.method_decls
                        .get(&(s, method_name.clone()))
                        .copied()
                } else {
                    let mut matching = self
                        .method_decls
                        .iter()
                        .filter(|((_, m), _)| m == method_name)
                        .map(|(_, d)| *d);
                    let first = matching.next()?;
                    if matching.next().is_some() {
                        None
                    } else {
                        Some(first)
                    }
                }?;
                let body = decl.body.as_ref()?;
                let mut slots = Vec::new();
                let mut bound: HashSet<String> =
                    decl.params.iter().map(|p| p.name.clone()).collect();
                bound.extend(decl.results.iter().filter_map(|r| r.name.clone()));
                if let Some(r) = &decl.receiver {
                    bound.insert(r.name.clone());
                    slots.push((
                        r.name.clone(),
                        self.shape_of_type(&r.ty),
                        Some(r.ty.clone()),
                        CalleeSlotSource::Arg(base.as_ref()),
                    ));
                }
                for (i, p) in decl.params.iter().enumerate() {
                    if let Some(arg) = cr.args.get(i) {
                        slots.push((
                            p.name.clone(),
                            self.shape_of_type(&p.ty),
                            Some(p.ty.clone()),
                            CalleeSlotSource::Arg(&arg.value),
                        ));
                    }
                }
                self.append_outer_binding_slots(st, &mut bound, false, &mut slots);
                Some((slots, std::borrow::Cow::Borrowed(body)))
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut slots = Vec::new();
                let mut bound: HashSet<String> =
                    params.iter().map(|p| p.name.clone()).collect();
                bound.extend(results.iter().filter_map(|r| r.name.clone()));
                for (i, p) in params.iter().enumerate() {
                    if let Some(arg) = cr.args.get(i) {
                        slots.push((
                            p.name.clone(),
                            self.shape_of_type(&p.ty),
                            Some(p.ty.clone()),
                            CalleeSlotSource::Arg(&arg.value),
                        ));
                    }
                }
                self.append_outer_binding_slots(st, &mut bound, true, &mut slots);
                Some((slots, std::borrow::Cow::Owned(body.clone())))
            }
            _ => None,
        }
    }

    fn collect_callee_param_stores(
        &self,
        slots: &[(&str, TypeShape, Option<&hy_syntax::TypeExpr>)],
        body: &Block,
        depth: usize,
    ) -> Vec<(usize, usize)> {
        if depth > 4 || slots.len() < 2 {
            return Vec::new();
        }
        let mut cst = FnState::default();
        let mut aliases: std::collections::HashMap<String, HashSet<usize>> =
            std::collections::HashMap::new();
        let mut param_names: HashSet<String> = HashSet::new();
        for (idx, (name, slot_shape, ty)) in slots.iter().enumerate() {
            let shape = if !matches!(slot_shape, TypeShape::Unknown) {
                slot_shape.clone()
            } else {
                ty.map(|t| self.shape_of_type(t))
                    .unwrap_or(TypeShape::Unknown)
            };
            cst.env_define(
                name,
                Binding {
                    shape,
                    ty: ty.cloned(),
                    immutable: false,
                    is_let: false,
                    managed_taint: false,
                    moved: None,
                    non_owned_origin: None,
                    slot_loan: false,
                },
            );
            aliases.insert((*name).to_string(), HashSet::from([idx]));
            param_names.insert((*name).to_string());
        }
        let mut out: HashSet<(usize, usize)> = HashSet::new();
        self.walk_block_param_stores(
            body,
            &mut cst,
            &mut aliases,
            &param_names,
            depth,
            &mut out,
        );
        let mut pairs: Vec<(usize, usize)> = out.into_iter().collect();
        pairs.sort_unstable();
        pairs
    }

    fn raw_expr_slot_origins(
        expr: &Expr,
        aliases: &std::collections::HashMap<String, HashSet<usize>>,
    ) -> HashSet<usize> {
        let mut roots = Vec::new();
        let inner = crate::model::unwrap_parens(expr);
        collect_ref_roots(inner, &mut roots);
        collect_addr_of_roots(inner, &mut roots);
        let mut move_ops = Vec::new();
        crate::model::collect_move_operands(inner, &mut move_ops);
        for (op, _) in move_ops {
            if let Expr::Ident { name, .. } = op {
                roots.push(name.clone());
            } else if let Some(r) = op.root_ident() {
                roots.push(r.to_string());
            }
        }
        let mut set = HashSet::new();
        for r in roots {
            if let Some(origins) = aliases.get(&r) {
                set.extend(origins.iter().copied());
            }
        }
        set
    }

    fn expr_slot_origins(
        &self,
        cst: &FnState,
        expr: &Expr,
        aliases: &std::collections::HashMap<String, HashSet<usize>>,
    ) -> HashSet<usize> {
        let inner = crate::model::unwrap_parens(expr);
        if self.call_returns_fresh_copy(cst, inner) {
            return HashSet::new();
        }
        let shape = self.shape_of_expr(cst, inner);
        if !self.shape_may_share(&shape) && !matches!(shape, TypeShape::Unknown) {
            return HashSet::new();
        }
        Self::raw_expr_slot_origins(inner, aliases)
    }

    fn walk_block_param_stores(
        &self,
        block: &Block,
        cst: &mut FnState,
        aliases: &mut std::collections::HashMap<String, HashSet<usize>>,
        param_names: &HashSet<String>,
        depth: usize,
        out: &mut HashSet<(usize, usize)>,
    ) {
        for s in &block.stmts {
            self.walk_stmt_param_stores(s, cst, aliases, param_names, depth, out);
        }
    }

    fn walk_stmt_param_stores(
        &self,
        stmt: &Stmt,
        cst: &mut FnState,
        aliases: &mut std::collections::HashMap<String, HashSet<usize>>,
        param_names: &HashSet<String>,
        depth: usize,
        out: &mut HashSet<(usize, usize)>,
    ) {
        // 1. Nested calls inside `stmt` (including `copy(dst, src)` and transitive helper calls)
        let mut calls = Vec::new();
        if let Stmt::For {
            kind: hy_syntax::ForKind::Iterate { expr, .. },
            ..
        } = stmt
        {
            walk::expr_calls(expr, &mut calls);
        } else {
            walk::own_calls(stmt, &mut calls);
        }
        for cr in &calls {
            self.walk_call_param_stores(cr, cst, aliases, param_names, depth, out);
        }

        // 2. Statement-specific alias & store updates
        match stmt {
            Stmt::Local(l) => {
                if l.values.is_empty() {
                    let shape = l
                        .ty
                        .as_ref()
                        .map(|t| self.shape_of_type(t))
                        .unwrap_or(TypeShape::Unknown);
                    for name in &l.names {
                        cst.env_define(
                            name,
                            Binding {
                                shape: shape.clone(),
                                ty: l.ty.clone(),
                                immutable: false,
                                is_let: l.kind == LocalKind::Let,
                                managed_taint: false,
                                moved: None,
                                non_owned_origin: None,
                                slot_loan: false,
                            },
                        );
                        aliases.remove(name);
                    }
                } else if l.names.len() == l.values.len() {
                    let rhs_info: Vec<(TypeShape, Option<hy_syntax::TypeExpr>, HashSet<usize>)> = l
                        .values
                        .iter()
                        .map(|v| {
                            let inferred = self.shape_of_expr(cst, v);
                            let (shape, ty) = match (&l.ty, inferred) {
                                (Some(t), TypeShape::Unknown) => {
                                    (self.shape_of_type(t), Some(t.clone()))
                                }
                                (_, inf) => (inf, self.ty_of_expr(cst, v).or_else(|| l.ty.clone())),
                            };
                            let origins = if !self.call_returns_fresh_copy(cst, v)
                                && (self.shape_may_share(&shape)
                                    || matches!(shape, TypeShape::Unknown))
                            {
                                Self::raw_expr_slot_origins(v, aliases)
                            } else {
                                HashSet::new()
                            };
                            (shape, ty, origins)
                        })
                        .collect();
                    for ((name, v), (shape, ty, origins)) in
                        l.names.iter().zip(l.values.iter()).zip(rhs_info)
                    {
                        cst.env_define(
                            name,
                            Binding {
                                shape,
                                ty,
                                immutable: false,
                                is_let: l.kind == LocalKind::Let,
                                managed_taint: false,
                                moved: None,
                                non_owned_origin: None,
                                slot_loan: false,
                            },
                        );
                        cst.closure_defs.remove(name);
                        self.record_closure_mutations(cst, name, v);
                        if origins.is_empty() {
                            aliases.remove(name);
                        } else {
                            aliases.insert(name.clone(), origins);
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let shapes = self.multi_value_shapes(cst, &l.values[0], l.names.len());
                    let tys = self.multi_value_tys(cst, &l.values[0], l.names.len());
                    let raw = Self::raw_expr_slot_origins(&l.values[0], aliases);
                    for ((name, shape), ty) in l.names.iter().zip(shapes).zip(tys) {
                        let carries =
                            self.shape_may_share(&shape) || matches!(shape, TypeShape::Unknown);
                        cst.env_define(
                            name,
                            Binding {
                                shape,
                                ty,
                                immutable: false,
                                is_let: l.kind == LocalKind::Let,
                                managed_taint: false,
                                moved: None,
                                non_owned_origin: None,
                                slot_loan: false,
                            },
                        );
                        if carries && !raw.is_empty() {
                            aliases.insert(name.clone(), raw.clone());
                        } else {
                            aliases.remove(name);
                        }
                    }
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                if targets.len() == values.len() {
                    let rhs_info: Vec<(TypeShape, Option<hy_syntax::TypeExpr>, HashSet<usize>)> =
                        values
                            .iter()
                            .map(|v| {
                                let shape = self.shape_of_expr(cst, v);
                                let ty = self.ty_of_expr(cst, v);
                                let origins = self.expr_slot_origins(cst, v, aliases);
                                (shape, ty, origins)
                            })
                            .collect();
                    for ((t, v), (v_shape, v_ty, src_slots)) in
                        targets.iter().zip(values.iter()).zip(rhs_info)
                    {
                        match &t.kind {
                            PlaceKind::Ident(name) => {
                                if let Some(b) = cst.env_lookup_mut(name) {
                                    if !matches!(v_shape, TypeShape::Unknown) || b.ty.is_none() {
                                        b.shape = v_shape;
                                    }
                                    if v_ty.is_some() || b.ty.is_none() {
                                        b.ty = v_ty;
                                    }
                                }
                                cst.closure_defs.remove(name);
                                self.record_closure_mutations(cst, name, v);
                                if src_slots.is_empty() {
                                    aliases.remove(name);
                                } else {
                                    aliases.insert(name.clone(), src_slots);
                                }
                            }
                            PlaceKind::Index { .. } | PlaceKind::Field { .. } => {
                                let t_shape =
                                    self.place_shape(cst, t).unwrap_or(TypeShape::Unknown);
                                let tgt_shares = self.shape_may_share(&t_shape)
                                    || matches!(t_shape, TypeShape::Unknown);
                                if tgt_shares && !src_slots.is_empty() {
                                    let t_root = t.root();
                                    if let Some(dst_slots) = aliases.get(t_root).cloned() {
                                        for d in &dst_slots {
                                            for s in &src_slots {
                                                if d != s {
                                                    out.insert((*d, *s));
                                                }
                                            }
                                        }
                                    }
                                    if !t_root.is_empty() && !param_names.contains(t_root) {
                                        aliases
                                            .entry(t_root.to_string())
                                            .or_default()
                                            .extend(src_slots);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                } else if values.len() == 1 && targets.len() > 1 {
                    let shapes = self.multi_value_shapes(cst, &values[0], targets.len());
                    let raw = Self::raw_expr_slot_origins(&values[0], aliases);
                    for (t, shape) in targets.iter().zip(shapes) {
                        let carries =
                            self.shape_may_share(&shape) || matches!(shape, TypeShape::Unknown);
                        let src_slots = if carries { raw.clone() } else { HashSet::new() };
                        match &t.kind {
                            PlaceKind::Ident(name) => {
                                if src_slots.is_empty() {
                                    aliases.remove(name);
                                } else {
                                    aliases.insert(name.clone(), src_slots);
                                }
                            }
                            PlaceKind::Index { .. } | PlaceKind::Field { .. } => {
                                let t_shape =
                                    self.place_shape(cst, t).unwrap_or(TypeShape::Unknown);
                                let tgt_shares = self.shape_may_share(&t_shape)
                                    || matches!(t_shape, TypeShape::Unknown);
                                if tgt_shares && !src_slots.is_empty() {
                                    let t_root = t.root();
                                    if let Some(dst_slots) = aliases.get(t_root).cloned() {
                                        for d in &dst_slots {
                                            for s in &src_slots {
                                                if d != s {
                                                    out.insert((*d, *s));
                                                }
                                            }
                                        }
                                    }
                                    if !t_root.is_empty() && !param_names.contains(t_root) {
                                        aliases
                                            .entry(t_root.to_string())
                                            .or_default()
                                            .extend(src_slots);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                let mut branch_cst = cst.clone();
                let mut branch_aliases = aliases.clone();
                if let Some(i) = init {
                    self.walk_stmt_param_stores(
                        i,
                        &mut branch_cst,
                        &mut branch_aliases,
                        param_names,
                        depth,
                        out,
                    );
                }
                let mut then_cst = branch_cst.clone();
                let mut then_aliases = branch_aliases.clone();
                self.walk_block_param_stores(
                    then,
                    &mut then_cst,
                    &mut then_aliases,
                    param_names,
                    depth,
                    out,
                );
                for (k, v) in then_aliases {
                    aliases.entry(k).or_default().extend(v);
                }
                if let Some(o) = otherwise {
                    let mut else_cst = branch_cst;
                    let mut else_aliases = branch_aliases;
                    self.walk_stmt_param_stores(
                        o,
                        &mut else_cst,
                        &mut else_aliases,
                        param_names,
                        depth,
                        out,
                    );
                    for (k, v) in else_aliases {
                        aliases.entry(k).or_default().extend(v);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                let mut body_cst = cst.clone();
                let mut body_aliases = aliases.clone();
                if let hy_syntax::ForKind::Iterate { vars, expr } = kind {
                    let shapes = self.iterate_var_shapes(&body_cst, expr, vars.len());
                    let raw = Self::raw_expr_slot_origins(expr, &body_aliases);
                    for (vname, vshape) in vars.iter().zip(shapes) {
                        let carries =
                            self.shape_may_share(&vshape) || matches!(vshape, TypeShape::Unknown);
                        body_cst.env_define(
                            vname,
                            Binding {
                                shape: vshape,
                                ty: None,
                                immutable: false,
                                is_let: false,
                                managed_taint: false,
                                moved: None,
                                non_owned_origin: None,
                                slot_loan: false,
                            },
                        );
                        if carries && !raw.is_empty() {
                            body_aliases.insert(vname.clone(), raw.clone());
                        } else {
                            body_aliases.remove(vname);
                        }
                    }
                }
                self.walk_block_param_stores(
                    body,
                    &mut body_cst,
                    &mut body_aliases,
                    param_names,
                    depth,
                    out,
                );
                for (k, v) in body_aliases {
                    if aliases.contains_key(&k) {
                        aliases.entry(k).or_default().extend(v);
                    }
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                self.walk_block_param_stores(body, cst, aliases, param_names, depth, out);
            }
            Stmt::Match { subject, arms, .. } => {
                let subj_origins = self.expr_slot_origins(cst, subject, aliases);
                let subj_shape = self.shape_of_expr(cst, subject);
                for arm in arms {
                    let mut arm_cst = cst.clone();
                    let mut arm_aliases = aliases.clone();
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(bname),
                        ..
                    } = &arm.pattern
                    {
                        arm_cst.env_define(
                            bname,
                            Binding {
                                shape: subj_shape.clone(),
                                ty: None,
                                immutable: false,
                                is_let: false,
                                managed_taint: false,
                                moved: None,
                                non_owned_origin: None,
                                slot_loan: false,
                            },
                        );
                        if subj_origins.is_empty() {
                            arm_aliases.remove(bname);
                        } else {
                            arm_aliases.insert(bname.clone(), subj_origins.clone());
                        }
                    }
                    self.walk_block_param_stores(
                        &arm.body,
                        &mut arm_cst,
                        &mut arm_aliases,
                        param_names,
                        depth,
                        out,
                    );
                    for (k, v) in arm_aliases {
                        if aliases.contains_key(&k) {
                            aliases.entry(k).or_default().extend(v);
                        }
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut arm_cst = cst.clone();
                    let mut arm_aliases = aliases.clone();
                    self.walk_block_param_stores(
                        &arm.body,
                        &mut arm_cst,
                        &mut arm_aliases,
                        param_names,
                        depth,
                        out,
                    );
                    for (k, v) in arm_aliases {
                        if aliases.contains_key(&k) {
                            aliases.entry(k).or_default().extend(v);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn walk_call_param_stores(
        &self,
        cr: &CallRef,
        cst: &FnState,
        aliases: &mut std::collections::HashMap<String, HashSet<usize>>,
        param_names: &HashSet<String>,
        depth: usize,
        out: &mut HashSet<(usize, usize)>,
    ) {
        if let Expr::Ident { name, .. } = crate::model::unwrap_parens(cr.callee) {
            if name == "copy" {
                let (Some(dst), Some(src)) = (cr.args.first(), cr.args.get(1)) else {
                    return;
                };
                let elem_shares = [&dst.value, &src.value].into_iter().any(|arg| {
                    match self.shape_of_expr(cst, crate::model::unwrap_parens(arg)) {
                        TypeShape::Slice(elem) => {
                            self.shape_may_share(&elem) || matches!(*elem, TypeShape::Unknown)
                        }
                        TypeShape::Unknown => true,
                        _ => false,
                    }
                });
                if !elem_shares {
                    return;
                }
                let dst_slots = Self::raw_expr_slot_origins(&dst.value, aliases);
                let src_slots = Self::raw_expr_slot_origins(&src.value, aliases);
                for d in &dst_slots {
                    for s in &src_slots {
                        if d != s {
                            out.insert((*d, *s));
                        }
                    }
                }
                if let Some(d_root) = dst.value.root_ident() {
                    if !param_names.contains(d_root) && !src_slots.is_empty() {
                        aliases
                            .entry(d_root.to_string())
                            .or_default()
                            .extend(src_slots);
                    }
                }
                return;
            }
        }
        if depth >= 4 {
            return;
        }
        let Some((sub_slots, sub_body)) = self.resolve_callee_slots_and_body(cst, cr) else {
            return;
        };
        if sub_slots.len() < 2 {
            return;
        }
        let sub_meta: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> = sub_slots
            .iter()
            .map(|(n, shape, ty, _)| (n.as_str(), shape.clone(), ty.as_ref()))
            .collect();
        let sub_pairs = self.collect_callee_param_stores(&sub_meta, &sub_body, depth + 1);
        for (sub_dst, sub_src) in sub_pairs {
            if sub_dst == sub_src {
                continue;
            }
            let (Some((_, _, _, dst_src)), Some((_, _, _, src_src))) =
                (sub_slots.get(sub_dst), sub_slots.get(sub_src))
            else {
                continue;
            };
            let dst_slots = match dst_src {
                CalleeSlotSource::Arg(dst_expr) => self.expr_slot_origins(cst, dst_expr, aliases),
                CalleeSlotSource::OuterBinding(name) => {
                    aliases.get(name).cloned().unwrap_or_default()
                }
            };
            let src_slots = match src_src {
                CalleeSlotSource::Arg(src_expr) => self.expr_slot_origins(cst, src_expr, aliases),
                CalleeSlotSource::OuterBinding(name) => {
                    aliases.get(name).cloned().unwrap_or_default()
                }
            };
            for d in &dst_slots {
                for s in &src_slots {
                    if d != s {
                        out.insert((*d, *s));
                    }
                }
            }
            let d_root_opt = match dst_src {
                CalleeSlotSource::Arg(dst_expr) => dst_expr.root_ident(),
                CalleeSlotSource::OuterBinding(name) => Some(name.as_str()),
            };
            if let Some(d_root) = d_root_opt {
                if !param_names.contains(d_root) && !src_slots.is_empty() {
                    aliases
                        .entry(d_root.to_string())
                        .or_default()
                        .extend(src_slots);
                }
            }
        }
    }

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
    pub(crate) fn record_alias(&self, st: &mut FnState, name: &str, value: &Expr) {
        if name == "_" {
            return;
        }
        match unwrap_parens(value) {
            Expr::Ident { name: src, .. } => {
                if let Some(b) = self.lookup_binding(st, src) {
                    let src_origin = b.non_owned_origin.clone();
                    if b.shape.is_reference_alias() || b.shape.is_handle() {
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
            other => {
                if self.call_returns_fresh_copy(st, value) {
                    return;
                }
                let mut other_shape = self.shape_of_expr(st, unwrap_parens(other));
                if matches!(other_shape, TypeShape::Unknown) {
                    if let Some(b) = self.lookup_binding(st, name) {
                        other_shape = b.shape.clone();
                    }
                }
                if other_shape.is_reference_alias()
                    || matches!(other_shape, TypeShape::OwnHandle(_))
                {
                    let mut roots = Vec::new();
                    collect_ref_roots(value, &mut roots);
                    self.collect_callee_const_return_roots(st, value, &mut roots, 0);
                    let mut borrowed_roots = Vec::new();
                    self.collect_borrowed_roots(st, value, &mut borrowed_roots);
                    for src in roots {
                        if src == name {
                            continue;
                        }
                        let matched = self.lookup_binding(st, &src).map(|b| {
                            let ok = if matches!(other_shape, TypeShape::OwnHandle(_)) {
                                self.shape_carries_handle(&b.shape)
                            } else {
                                b.shape.is_reference_alias()
                                    || (borrowed_roots.iter().any(|r| r == &src)
                                        && self.binding_can_be_borrowed(st, &src))
                            };
                            (ok, b.non_owned_origin.clone())
                        });
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
            }
        }
    }

    /// Record O5.2 alias links for pointer borrows (`p := &stats`, `slot := &m[k]`)
    /// so moving `p` freezes the underlying mutable binding `stats` (#33).
    /// pr-312 / #131: the alias recording descends into nested address-of
    /// operands and auto-addressed `*T` call arguments/receivers via
    /// `collect_borrowed_roots`, not just the direct `as_addr_of` form.
    pub(crate) fn record_local_borrow_aliases(&self, st: &mut FnState, l: &LocalDecl) {
        let values: Vec<_> = if l.names.len() == l.values.len() {
            l.values
                .iter()
                .map(|v| (v, self.shape_of_expr(st, v)))
                .collect()
        } else if l.values.len() == 1 && l.names.len() > 1 {
            self.multi_value_shapes(st, &l.values[0], l.names.len())
                .into_iter()
                .map(|shape| (&l.values[0], shape))
                .collect()
        } else {
            return;
        };
        for (name, (value, shape)) in l.names.iter().zip(values) {
            if name == "_" {
                continue;
            }
            let borrow_slot = field_borrow_kind(&shape).is_some() || matches!(shape, TypeShape::Unknown);
            if (l.names.len() == l.values.len() || borrow_slot) && self.expr_carries_borrow(st, value) {
                st.borrow_bindings.insert(name.clone());
            } else {
                st.borrow_bindings.remove(name);
            }
            if !borrow_slot || move_operand_ident(value).is_some() {
                continue;
            }
            let mut addr_roots = Vec::new();
            self.collect_borrowed_roots(st, value, &mut addr_roots);
            for src in addr_roots {
                if src == *name {
                    continue;
                }
                let matched = self.lookup_binding(st, &src).map(|b| {
                    let ok = !b.shape.is_sendable_by_type()
                        || b.immutable
                        || self.binding_can_be_borrowed(st, &src);
                    (ok, b.non_owned_origin.clone())
                });
                if let Some((true, origin)) = matched {
                    st.share_edge(name, &src);
                    st.aliases.insert(name.clone(), src);
                    if let Some(origin) = origin {
                        if let Some(nb) = st.env_lookup_mut(name) {
                            if nb.non_owned_origin.is_none() {
                                nb.non_owned_origin = Some(origin);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Record O5.2 alias links for pointer-borrow rebinds (`p = &stats`).
    /// pr-312 / #131: nested address-of operands and auto-addressed `*T`
    /// call arguments/receivers are tracked the same way as the define form.
    pub(crate) fn record_assign_borrow_aliases(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) {
        if op == AssignOp::Assign {
            let shaped_values: Vec<_> = if values.len() == targets.len() {
                values
                    .iter()
                    .map(|v| (v, self.shape_of_expr(st, v)))
                    .collect()
            } else if values.len() == 1 && targets.len() > 1 {
                self.multi_value_shapes(st, &values[0], targets.len())
                    .into_iter()
                    .map(|shape| (&values[0], shape))
                    .collect()
            } else {
                return;
            };
            for (t, (v, shape)) in targets.iter().zip(shaped_values) {
                let carries_borrow = self.expr_carries_borrow(st, v);
                if let PlaceKind::Ident(name) = &t.kind {
                    if name == "_" {
                        continue;
                    }
                    let borrow_slot =
                        field_borrow_kind(&shape).is_some() || matches!(shape, TypeShape::Unknown);
                    let inner = unwrap_parens(v);
                    let is_borrow = as_addr_of(inner).is_some()
                        || contains_addr_of_in_literal(inner)
                        || matches!(inner, Expr::Ident { name: src, .. } if st.borrow_bindings.contains(src));
                    if (values.len() == targets.len() || borrow_slot) && (is_borrow || carries_borrow)
                    {
                        st.borrow_bindings.insert(name.clone());
                    } else {
                        st.borrow_bindings.remove(name);
                    }
                    if borrow_slot && move_operand_ident(v).is_none() {
                        let mut addr_roots = Vec::new();
                        self.collect_borrowed_roots(st, v, &mut addr_roots);
                        for src in addr_roots {
                            if src != *name {
                                let matched = self.lookup_binding(st, &src).map(|b| {
                                    let ok = !b.shape.is_sendable_by_type()
                                        || b.immutable
                                        || self.binding_can_be_borrowed(st, &src);
                                    (ok, b.non_owned_origin.clone())
                                });
                                if let Some((true, origin)) = matched {
                                    st.share_edge(name, &src);
                                    st.aliases.insert(name.clone(), src);
                                    if let Some(origin) = origin {
                                        if let Some(nb) = st.env_lookup_mut(name) {
                                            if nb.non_owned_origin.is_none() {
                                                nb.non_owned_origin = Some(origin);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let root = t.root();
                    if !root.is_empty() && carries_borrow {
                        st.borrow_bindings.insert(root.to_string());
                        if move_operand_ident(v).is_none() {
                            let mut addr_roots = Vec::new();
                            self.collect_expr_addr_of_roots(st, v, &mut addr_roots);
                            for src in addr_roots {
                                if src != root {
                                    if let Some(b) = self.lookup_binding(st, &src) {
                                        if !b.shape.is_sendable_by_type() {
                                            st.aliases.entry(root.to_string()).or_insert(src);
                                            break;
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
        let view_last = st.last_use.get(name).copied().unwrap_or(idx).max(idx);
        let mut calls = Vec::new();
        walk::expr_calls(value, &mut calls);
        let mut roots = Vec::new();
        for cr in &calls {
            match self.resolve_call(st, cr) {
                Resolved::Method {
                    root, method, recv, ..
                } => {
                    if let Some(t) = receiver_type_name(&recv) {
                        if foreign::method_mode(&t, &method) == CallMode::Borrow {
                            roots.push(root);
                        }
                    }
                }
                Resolved::Package { pkg, method } => {
                    if foreign::package_mode(&pkg, &method) == CallMode::Borrow {
                        for a in cr.args {
                            if let Some(root) = a.value.root_ident() {
                                let arg_is_handle = self.shape_of_expr(st, &a.value).is_handle();
                                if self
                                    .lookup_binding(st, root)
                                    .map(|b| b.shape.is_handle() || arg_is_handle)
                                    .unwrap_or(false)
                                {
                                    roots.push(root.to_string());
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        for loan in st.loans.iter_mut() {
            if let LoanTarget::Handle { handle } = &loan.target {
                if loan.created == idx && loan.last == idx && roots.iter().any(|r| r == handle) {
                    loan.last = view_last;
                }
            }
        }
        let mut ref_roots = Vec::new();
        collect_ref_roots(value, &mut ref_roots);
        for src in ref_roots {
            if src == name {
                continue;
            }
            let is_borrow_src = self
                .lookup_binding(st, &src)
                .map(|b| matches!(b.shape, TypeShape::BorrowHandle(_)))
                .unwrap_or(false);
            if is_borrow_src {
                let group = st.share_group(&src);
                for loan in st.loans.iter_mut() {
                    if let LoanTarget::Handle { handle } = &loan.target {
                        if loan.created <= idx
                            && idx <= loan.last
                            && (handle == &src || group.contains(handle))
                        {
                            loan.last = loan.last.max(view_last);
                        }
                    }
                }
            }
        }
    }

    /// Record may-share edges for one target `name` (or non-Ident root) of a
    /// multi-value define or assignment using that target's resolved `shape`.
    /// Multi-value define share/alias recording (#457): a multi-value
    /// define (`a, b := f()`) whose value shares storage records may-share
    /// edges and reference-alias links for EACH named result against the
    /// value's ref roots — including the outer-scope shadow link for a
    /// define that shadows a captured reference alias.
    pub(crate) fn record_multi_value_share_and_alias(
        &self,
        st: &mut FnState,
        name: &str,
        shape: &TypeShape,
        value: &Expr,
    ) {
        if !self.shape_may_share(shape) && !matches!(shape, TypeShape::Unknown) {
            return;
        }
        self.record_multi_value_may_share(st, name, shape, value);
        let mut roots = Vec::new();
        collect_ref_roots(value, &mut roots);
        self.collect_callee_const_return_roots(st, value, &mut roots, 0);
        if shape.is_reference_alias() || matches!(shape, TypeShape::OwnHandle(_)) {
            for src in &roots {
                if src == name {
                    let outer_b = if !st.scopes.is_empty() {
                        st.scopes
                            .iter()
                            .take(st.scopes.len().saturating_sub(1))
                            .rev()
                            .find_map(|s| s.get(name))
                            .or_else(|| st.env.get(name))
                            .cloned()
                    } else {
                        None
                    };
                    if let Some(ob) = outer_b {
                        let ok = if matches!(shape, TypeShape::OwnHandle(_)) {
                            self.shape_carries_handle(&ob.shape)
                        } else {
                            self.shape_may_share(&ob.shape)
                        };
                        if ok {
                            st.aliases.insert(name.to_string(), src.clone());
                            if let Some(origin) = ob.non_owned_origin {
                                if let Some(nb) = st.env_lookup_mut(name) {
                                    nb.non_owned_origin = Some(origin);
                                }
                            }
                            break;
                        }
                    }
                    continue;
                }
                let matched = self.lookup_binding(st, src).map(|b| {
                    let ok = if matches!(shape, TypeShape::OwnHandle(_)) {
                        self.shape_carries_handle(&b.shape)
                    } else {
                        b.shape.is_reference_alias()
                    };
                    (ok, b.non_owned_origin.clone())
                });
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
    }

    pub(crate) fn record_multi_value_may_share(
        &self,
        st: &mut FnState,
        name: &str,
        shape: &TypeShape,
        value: &Expr,
    ) {
        if !self.shape_may_share(shape) && !matches!(shape, TypeShape::Unknown) {
            return;
        }
        let mut roots = Vec::new();
        collect_ref_roots(value, &mut roots);
        self.collect_callee_const_return_roots(st, value, &mut roots, 0);
        let mut borrowed_roots = Vec::new();
        self.collect_borrowed_roots(st, value, &mut borrowed_roots);
        for root in &roots {
            if root == name {
                let outer_may_share = !st.scopes.is_empty()
                    && st
                        .scopes
                        .iter()
                        .take(st.scopes.len().saturating_sub(1))
                        .rev()
                        .find_map(|s| s.get(name))
                        .or_else(|| st.env.get(name))
                        .map(|b| {
                            self.shape_may_share(&b.shape)
                                || self.expr_slices_addressable_array(st, value, root)
                        })
                        .unwrap_or(false);
                if outer_may_share {
                    st.share_edge(name, root);
                }
                continue;
            }
            if let Some(b) = self.lookup_binding(st, root) {
                if b.shape.is_handle()
                    && !self.shape_carries_handle(shape)
                    && !matches!(shape, TypeShape::Unknown)
                {
                    continue;
                }
                if self.shape_may_share(&b.shape)
                    || self.expr_slices_addressable_array(st, value, root)
                {
                    st.share_edge(name, root);
                }
            }
        }
        for root in &borrowed_roots {
            if root != name {
                st.share_edge(name, root);
                if let Some(origin) = self
                    .lookup_binding(st, root)
                    .and_then(|b| b.non_owned_origin.clone())
                {
                    if let Some(nb) = st.env_lookup_mut(name) {
                        if nb.non_owned_origin.is_none() {
                            nb.non_owned_origin = Some(origin);
                        }
                    }
                }
            }
        }
        if shape.is_reference_alias() || matches!(shape, TypeShape::OwnHandle(_)) {
            for src in &roots {
                if src == name {
                    let outer_b = if !st.scopes.is_empty() {
                        st.scopes
                            .iter()
                            .take(st.scopes.len().saturating_sub(1))
                            .rev()
                            .find_map(|s| s.get(name))
                            .or_else(|| st.env.get(name))
                            .cloned()
                    } else {
                        None
                    };
                    if let Some(ob) = outer_b {
                        let ok = if matches!(shape, TypeShape::OwnHandle(_)) {
                            self.shape_carries_handle(&ob.shape)
                        } else {
                            self.shape_may_share(&ob.shape)
                        };
                        if ok {
                            st.aliases.insert(name.to_string(), src.clone());
                            if let Some(origin) = ob.non_owned_origin {
                                if let Some(nb) = st.env_lookup_mut(name) {
                                    nb.non_owned_origin = Some(origin);
                                }
                            }
                            break;
                        }
                    }
                    continue;
                }
                let matched = self.lookup_binding(st, src).map(|b| {
                    let ok = if matches!(shape, TypeShape::OwnHandle(_)) {
                        self.shape_carries_handle(&b.shape)
                    } else {
                        b.shape.is_reference_alias()
                            || (borrowed_roots.iter().any(|r| r == src)
                                && self.binding_can_be_borrowed(st, src))
                    };
                    (ok, b.non_owned_origin.clone())
                });
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
    }

    /// The results of ONE call can alias each other: `return m, m` hands the
    /// caller two bindings over the same storage. Each result is bound (and
    /// frozen) per name, so without a share edge here `move a` freezes `a`
    /// alone while `b` stays live on the same map — the E3003 shape, accepted.
    ///
    /// The callee is in the same file (`self.fns`), so its `return` positions
    /// are compared by their rendered form: equal roots mean the two results
    /// may share storage, and the pair joins the caller's may-share graph,
    /// which the freeze rule already follows.
    pub(crate) fn link_multi_value_return_aliases(
        &self,
        st: &mut FnState,
        value: &Expr,
        names: &[String],
    ) {
        if names.len() < 2 {
            return;
        }
        for (i, j) in self.callee_shared_return_pairs(st, value) {
            let (Some(a), Some(b)) = (names.get(i), names.get(j)) else {
                continue;
            };
            if a == b || a == "_" || b == "_" {
                continue;
            }
            let a_shares = self
                .lookup_binding(st, a)
                .map(|x| self.shape_may_share(&x.shape))
                .unwrap_or(false);
            let b_shares = self
                .lookup_binding(st, b)
                .map(|x| self.shape_may_share(&x.shape))
                .unwrap_or(false);
            if !a_shares || !b_shares {
                continue;
            }
            st.share_edge(a, b);
            st.aliases.entry(a.clone()).or_insert_with(|| b.clone());
            st.aliases.entry(b.clone()).or_insert_with(|| a.clone());
        }
    }

    /// Resolve a call expression's callee to `(typed_params, typed_results, body)`
    /// across top-level functions (`self.fns`), user-defined struct methods
    /// (`self.method_decls`), inline function literals (`Expr::FnLit`), and
    /// local closure bindings (`st.closure_fnlits`).
    fn resolve_call_expr_decl_or_fnlit<'c>(
        &self,
        st: &'c FnState,
        callee: &'c Expr,
    ) -> Option<(
        Vec<(String, Option<hy_syntax::TypeExpr>)>,
        Vec<(String, Option<hy_syntax::TypeExpr>)>,
        &'c Block,
    )>
    where
        'a: 'c,
    {
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if let Some((params, results, body)) = st.closure_fnlits.get(name.as_str()) {
                    return Some((params.clone(), results.clone(), body));
                }
                if self.lookup_binding(st, name).is_some() {
                    return None;
                }
                let head = Self::base_named_type_str(name);
                let decl = self
                    .fns
                    .get(name.as_str())
                    .or_else(|| self.fns.get(head))
                    .copied()?;
                let body = decl.body.as_ref()?;
                let params = decl
                    .params
                    .iter()
                    .map(|p| (p.name.clone(), Some(p.ty.clone())))
                    .collect();
                let results = decl
                    .results
                    .iter()
                    .map(|r| (r.name.clone().unwrap_or_default(), Some(r.ty.clone())))
                    .collect();
                Some((params, results, body))
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() && self.known_package(pkg) {
                        return None;
                    }
                }
                let head = Self::base_named_type_str(method_name);
                let recv_struct = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match self.shape_of_expr(st, base) {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(&s).to_string())
                        }
                        TypeShape::Pointer(inner) => match *inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(&s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                let decl = if let Some(s) = recv_struct {
                    self.method_decls.get(&(s, head.to_string())).copied()
                } else {
                    let mut matching = self
                        .method_decls
                        .iter()
                        .filter(|((_, m), _)| m == head)
                        .map(|(_, d)| *d);
                    let first = matching.next()?;
                    if matching.next().is_some() {
                        None
                    } else {
                        Some(first)
                    }
                }?;
                let body = decl.body.as_ref()?;
                let mut params = Vec::new();
                if let Some(r) = &decl.receiver {
                    params.push((r.name.clone(), Some(r.ty.clone())));
                }
                params.extend(
                    decl.params
                        .iter()
                        .map(|p| (p.name.clone(), Some(p.ty.clone()))),
                );
                let results = decl
                    .results
                    .iter()
                    .map(|r| (r.name.clone().unwrap_or_default(), Some(r.ty.clone())))
                    .collect();
                Some((params, results, body))
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let p = params
                    .iter()
                    .map(|p| (p.name.clone(), Some(p.ty.clone())))
                    .collect();
                let r = results
                    .iter()
                    .map(|r| (r.name.clone().unwrap_or_default(), Some(r.ty.clone())))
                    .collect();
                Some((p, r, body))
            }
            _ => None,
        }
    }

    /// Collect any file-level `const` composite roots (`self.consts` whose
    /// shape `shape_may_share` is true) that a call inside `value` may return
    /// (directly, via local aliases/projections/literals, or via named results
    /// on bare `return`).
    ///
    /// Fails closed (issue #2320): a chain of calls deeper than
    /// `CALLEE_CONST_RETURN_WALK_LIMIT` and a callee whose body this file
    /// cannot see (a function value in a binding, a computed callee) report
    /// every may-share file-level `const` instead of assuming a fresh result.
    /// The report only covers results that may share (or whose shape did not
    /// resolve): a result the checker types as a plain value cannot alias a
    /// `const`, so it keeps the pre-#2320 verdict.
    pub(crate) fn collect_callee_const_return_roots(
        &self,
        st: &FnState,
        value: &Expr,
        out: &mut Vec<String>,
        depth: usize,
    ) {
        let mut walked: HashMap<String, Option<Vec<String>>> = HashMap::new();
        self.walk_callee_const_return_roots(st, value, out, depth, &mut walked);
    }

    /// Depth bound for the interprocedural walk in
    /// `collect_callee_const_return_roots`. Chains deeper than this are not
    /// traced: past the bound the walk reports every may-share file-level
    /// `const` instead of assuming the call result is fresh (SPEC O4.9). The
    /// visited-set below keeps one walk linear in the number of callee bodies,
    /// so the bound is a safety net against pathological inputs, not a cost
    /// limit (#2320).
    const CALLEE_CONST_RETURN_WALK_LIMIT: usize = 32;

    /// Is a fail-closed report warranted for `value`? SPEC O4.9 is about the
    /// share, not about the name: only a result that may share mutable GC
    /// storage (or whose shape did not resolve) can hand a file-level `const`
    /// back, so an expression the checker types as a plain value — a deep
    /// chain of `int`-returning wrappers, say — keeps its report off. The
    /// shape is only trusted where it did resolve; an unresolved shape stays
    /// fail-closed through the `Unknown` arm.
    fn fail_closed_for(&self, st: &FnState, value: &Expr) -> bool {
        let shape = self.shape_of_expr(st, unwrap_parens(value));
        self.shape_may_share(&shape) || matches!(shape, TypeShape::Unknown)
    }

    /// Fail closed (SPEC O4.9, GUARANTEES row 4): every file-level `const`
    /// whose shape may share is a possible value of an expression whose callee
    /// this file cannot trace. Callers intersect these names with their own
    /// roots, so reporting a superset is always safe.
    fn push_may_share_const_roots(&self, out: &mut Vec<String>) {
        for (name, cb) in &self.consts {
            if self.shape_may_share(&cb.shape) && !out.contains(name) {
                out.push(name.clone());
            }
        }
    }

    /// A callee whose body this file cannot see, but which may still return a
    /// file-level `const` of this file (SPEC O4.9 — fail closed):
    ///
    /// * a function value held in a local binding or file-level `const`
    ///   (`cb := leaf; m := cb()`), or a closure binding `st.closure_fnlits`
    ///   does not know;
    /// * a computed callee (`fns[i]()`, `pick()()`);
    /// * a field call on a value of a struct this file declares, when the name
    ///   is not a declared method: a func-typed struct field (`holder.cb()`) or
    ///   a method we could not attribute to one receiver (ambiguous shape).
    ///
    /// Deliberately NOT included: a call into another package (`pkg.Fn()`) and
    /// an unresolvable bare name (`make`, `append`, an undefined function).
    /// `const`s are file-level, so another file's function cannot hand one of
    /// ours back, and the builtins return fresh values.
    fn callee_returns_unseen_shared_const(&self, st: &FnState, callee: &Expr) -> bool {
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => self.lookup_binding(st, name).is_some(),
            Expr::Field {
                base,
                name: method,
                ..
            } => {
                if matches!(unwrap_parens(base), Expr::Ident { name: pkg, .. }
                    if self.lookup_binding(st, pkg).is_none() && self.known_package(pkg))
                {
                    return false;
                }
                let head = Self::base_named_type_str(method);
                if self.method_decls.keys().any(|(_, m)| m == head) {
                    return true;
                }
                // A field of a struct declared in this file may hold a function
                // value (`holder.cb()`), whose target we cannot see.
                match self.shape_of_expr(st, base) {
                    TypeShape::Struct(Some(s)) => self.structs.contains_key(&s),
                    TypeShape::Pointer(inner) => {
                        matches!(*inner, TypeShape::Struct(Some(ref s)) if self.structs.contains_key(s))
                    }
                    _ => false,
                }
            }
            Expr::Index { .. } | Expr::Call { .. } => true,
            _ => false,
        }
    }

    /// The walk behind `collect_callee_const_return_roots`.
    ///
    /// `walked` keys a callee by the address of its body block (`None` while
    /// that body is on the walk, `Some(roots)` once its contribution is known).
    /// Within one query a callee contributes the same roots no matter which
    /// call site asked for it, so a repeat visit replays the first one — that is
    /// what keeps a diamond-shaped call graph linear instead of exponential in
    /// the depth bound. A body that is already on the walk (recursion) is
    /// reported fail-closed: its own `return`s are being handled by the outer
    /// walk, and re-entering it would loop.
    fn walk_callee_const_return_roots(
        &self,
        st: &FnState,
        value: &Expr,
        out: &mut Vec<String>,
        depth: usize,
        walked: &mut HashMap<String, Option<Vec<String>>>,
    ) {
        if depth > Self::CALLEE_CONST_RETURN_WALK_LIMIT {
            if self.fail_closed_for(st, value) {
                self.push_may_share_const_roots(out);
            }
            return;
        }
        match unwrap_parens(value) {
            Expr::Move { .. } => {}
            Expr::Try { operand, .. }
            | Expr::Slice { base: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Index { base: operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                ..
            } => {
                self.walk_callee_const_return_roots(st, operand, out, depth, walked);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.walk_callee_const_return_roots(st, &f.value, out, depth, walked);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.walk_callee_const_return_roots(st, k, out, depth, walked);
                    self.walk_callee_const_return_roots(st, v, out, depth, walked);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.walk_callee_const_return_roots(st, it, out, depth, walked);
                }
            }
            Expr::Call { callee, args, .. } => {
                for a in args {
                    self.walk_callee_const_return_roots(st, &a.value, out, depth, walked);
                }
                self.walk_callee_const_return_roots(st, callee, out, depth, walked);
                let Some((typed_params, typed_results, body)) =
                    self.resolve_call_expr_decl_or_fnlit(st, callee)
                else {
                    // The callee's declaration is not visible here; if the
                    // value it is called through may still be one of our
                    // file-level `const`s, report them all instead of treating
                    // the call result as fresh (SPEC O4.9, issue #2320).
                    if self.callee_returns_unseen_shared_const(st, callee)
                        && self.fail_closed_for(st, value)
                    {
                        self.push_may_share_const_roots(out);
                    }
                    return;
                };
                let key = format!("{:p}", body as *const Block);
                match walked.get(&key) {
                    Some(Some(roots)) => {
                        for r in roots {
                            if !out.contains(r) {
                                out.push(r.clone());
                            }
                        }
                        return;
                    }
                    Some(None) => {
                        // A body already on the walk: its own returns are
                        // handled by the outer walk, and re-entering it would
                        // loop. Report fail-closed where a share is possible.
                        if self.fail_closed_for(st, value) {
                            self.push_may_share_const_roots(out);
                        }
                        return;
                    }
                    None => {}
                }
                walked.insert(key.clone(), None);
                let mark = out.len();
                let mut shadowed: HashSet<String> =
                    typed_params.iter().map(|(n, _)| n.clone()).collect();
                shadowed.extend(
                    typed_results
                        .iter()
                        .filter_map(|(n, _)| if !n.is_empty() { Some(n.clone()) } else { None }),
                );
                let mut aliases: HashMap<String, HashSet<String>> = HashMap::new();
                for (cname, cb) in &self.consts {
                    if self.shape_may_share(&cb.shape) && !shadowed.contains(cname) {
                        aliases.insert(cname.clone(), HashSet::from([cname.clone()]));
                    }
                }
                for (local_name, lb) in &st.env {
                    if !shadowed.contains(local_name) {
                        if let Some(croot) = self.const_write_root(
                            st,
                            &Place {
                                kind: PlaceKind::Ident(local_name.clone()),
                                span: value.span(),
                            },
                        ) {
                            if self.shape_may_share(&lb.shape) {
                                aliases.insert(local_name.clone(), HashSet::from([croot]));
                            }
                        }
                    }
                }
                let expr_const_origins =
                    |e: &Expr,
                     aliases: &HashMap<String, HashSet<String>>,
                     shadowed: &HashSet<String>,
                     walked: &mut HashMap<String, Option<Vec<String>>>|
                     -> HashSet<String> {
                        let mut roots = Vec::new();
                        collect_ref_roots(e, &mut roots);
                        self.walk_callee_const_return_roots(st, e, &mut roots, depth + 1, walked);
                        let mut set = HashSet::new();
                        for r in roots {
                            if let Some(cs) = aliases.get(&r) {
                                set.extend(cs.iter().cloned());
                            } else if !shadowed.contains(&r) {
                                if let Some(cb) = self.consts.get(&r) {
                                    if self.shape_may_share(&cb.shape) {
                                        set.insert(r);
                                    }
                                }
                            }
                        }
                        set
                    };
                let flat = walk::flatten(body);
                for (stmt, _) in &flat.stmts {
                    match stmt {
                        Stmt::Local(l) => {
                            if l.values.is_empty() {
                                for n in &l.names {
                                    aliases.remove(n);
                                    shadowed.insert(n.clone());
                                }
                            } else if l.values.len() == 1 && l.names.len() > 1 {
                                let cs =
                                    expr_const_origins(&l.values[0], &aliases, &shadowed, walked);
                                for n in &l.names {
                                    if cs.is_empty() {
                                        aliases.remove(n);
                                        shadowed.insert(n.clone());
                                    } else {
                                        shadowed.remove(n);
                                        aliases.insert(n.clone(), cs.clone());
                                    }
                                }
                            } else {
                                for (n, v) in l.names.iter().zip(l.values.iter()) {
                                    let cs = expr_const_origins(v, &aliases, &shadowed, walked);
                                    if cs.is_empty() {
                                        aliases.remove(n);
                                        shadowed.insert(n.clone());
                                    } else {
                                        shadowed.remove(n);
                                        aliases.insert(n.clone(), cs);
                                    }
                                }
                            }
                        }
                        Stmt::Assign {
                            targets, values, ..
                        } => {
                            if targets.len() == values.len() {
                                let mut rhs_cs: Vec<HashSet<String>> =
                                    Vec::with_capacity(values.len());
                                for v in values {
                                    rhs_cs.push(expr_const_origins(v, &aliases, &shadowed, walked));
                                }
                                for (t, cs) in targets.iter().zip(rhs_cs) {
                                    match &t.kind {
                                        PlaceKind::Ident(n) => {
                                            if cs.is_empty() {
                                                aliases.remove(n);
                                                shadowed.insert(n.clone());
                                            } else {
                                                shadowed.remove(n);
                                                aliases.insert(n.clone(), cs);
                                            }
                                        }
                                        PlaceKind::Field { .. } | PlaceKind::Index { .. } => {
                                            let root = t.root();
                                            if !root.is_empty() && !cs.is_empty() {
                                                aliases
                                                    .entry(root.to_string())
                                                    .or_default()
                                                    .extend(cs);
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            } else if values.len() == 1 && targets.len() > 1 {
                                let cs =
                                    expr_const_origins(&values[0], &aliases, &shadowed, walked);
                                for t in targets {
                                    if let PlaceKind::Ident(n) = &t.kind {
                                        if cs.is_empty() {
                                            aliases.remove(n);
                                            shadowed.insert(n.clone());
                                        } else {
                                            shadowed.remove(n);
                                            aliases.insert(n.clone(), cs.clone());
                                        }
                                    }
                                }
                            }
                        }
                        Stmt::Return { values, .. } => {
                            if values.is_empty() {
                                for (nr, _) in &typed_results {
                                    if nr.is_empty() {
                                        continue;
                                    }
                                    if let Some(cs) = aliases.get(nr) {
                                        for c in cs {
                                            if !out.contains(c) {
                                                out.push(c.clone());
                                            }
                                        }
                                    }
                                }
                            } else {
                                for v in values {
                                    for c in expr_const_origins(v, &aliases, &shadowed, walked) {
                                        if !out.contains(&c) {
                                            out.push(c);
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
                // The roots this callee contributed, for the replay path above.
                walked.insert(key, Some(out[mark..].to_vec()));
            }
            _ => {}
        }
    }

    fn collect_sharing_origins_expr(
        &self,
        cst: &FnState,
        e: &Expr,
        origins: &HashMap<String, HashSet<String>>,
        shadowed: &HashSet<String>,
        out: &mut HashSet<String>,
    ) {
        let inner = unwrap_parens(e);
        let is_lit = matches!(
            inner,
            Expr::StructLit { .. } | Expr::MapLit { .. } | Expr::SliceLit { .. }
        );
        let sh = self.shape_of_expr(cst, inner);
        if !is_lit
            && !self.shape_may_share(&sh)
            && !matches!(sh, TypeShape::Unknown | TypeShape::Array(_))
        {
            return;
        }
        match inner {
            Expr::Move { .. } => {}
            Expr::Ident { name, .. } => {
                if let Some(os) = origins.get(name) {
                    out.extend(os.iter().cloned());
                } else if !shadowed.contains(name) {
                    if let Some(b) = self.lookup_binding(cst, name) {
                        if self.shape_may_share(&b.shape) || matches!(b.shape, TypeShape::Array(_))
                        {
                            out.insert(format!("ident:{name}"));
                        }
                    }
                }
            }
            Expr::Try { operand, .. }
            | Expr::Slice { base: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Index { base: operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                ..
            } => {
                self.collect_sharing_origins_expr(cst, operand, origins, shadowed, out);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.collect_sharing_origins_expr(cst, &f.value, origins, shadowed, out);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.collect_sharing_origins_expr(cst, k, origins, shadowed, out);
                    self.collect_sharing_origins_expr(cst, v, origins, shadowed, out);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.collect_sharing_origins_expr(cst, it, origins, shadowed, out);
                }
            }
            Expr::Call { callee, args, .. } => {
                for a in args {
                    self.collect_sharing_origins_expr(cst, &a.value, origins, shadowed, out);
                }
                self.collect_sharing_origins_expr(cst, callee, origins, shadowed, out);
                let mut c_roots = Vec::new();
                self.collect_callee_const_return_roots(cst, inner, &mut c_roots, 0);
                for cr in c_roots {
                    out.insert(format!("ident:{cr}"));
                }
            }
            _ => {}
        }
    }

    /// Return-position pairs `(i, j)` whose values may share backing storage in
    /// a `return` of the called function, method, or closure (`return m, m`,
    /// `m2 := m; return m, m2`, `return s[0:1], s[1:2]`, `return box{m: m}, m`,
    /// or bare `return` with named results).
    pub(crate) fn callee_shared_return_pairs(&self, st: &FnState, value: &Expr) -> Vec<(usize, usize)> {
        let call = match unwrap_parens(value) {
            Expr::Try { operand, .. } => unwrap_parens(operand),
            other => other,
        };
        let Expr::Call { callee, .. } = call else {
            return Vec::new();
        };
        let Some((typed_params, typed_results, body)) =
            self.resolve_call_expr_decl_or_fnlit(st, callee)
        else {
            return Vec::new();
        };
        let mut cst = st.clone();
        cst.scope_push();
        let mut origins: HashMap<String, HashSet<String>> = HashMap::new();
        let mut shadowed: HashSet<String> = HashSet::new();
        for (p, pty) in &typed_params {
            let shape = pty
                .as_ref()
                .map(|t| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown);
            self.define_binding(&mut cst, p, shape.clone(), pty.clone(), false, false, false);
            if self.shape_may_share(&shape)
                || matches!(shape, TypeShape::Unknown | TypeShape::Array(_))
            {
                origins.insert(p.clone(), HashSet::from([format!("param:{p}")]));
            } else {
                shadowed.insert(p.clone());
            }
        }
        for (nr, rty) in &typed_results {
            if nr == "_" || nr.is_empty() {
                continue;
            }
            let shape = rty
                .as_ref()
                .map(|t| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown);
            self.define_binding(&mut cst, nr, shape, rty.clone(), false, false, false);
            origins.remove(nr);
            shadowed.insert(nr.clone());
        }

        let flat = walk::flatten(body);
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (stmt, idx) in flat.stmts.iter().copied() {
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            for cr in &calls {
                if let Expr::Ident { name, .. } = unwrap_parens(cr.callee) {
                    if name == "copy" && cr.args.len() >= 2 {
                        let mut src_o = HashSet::new();
                        self.collect_sharing_origins_expr(
                            &cst,
                            &cr.args[1].value,
                            &origins,
                            &shadowed,
                            &mut src_o,
                        );
                        let mut dst_roots = Vec::new();
                        collect_ref_roots(&cr.args[0].value, &mut dst_roots);
                        for dr in dst_roots {
                            if !src_o.is_empty() {
                                origins.entry(dr).or_default().extend(src_o.iter().cloned());
                            }
                        }
                    }
                }
                if let Some((slots, cbody)) = self.resolve_callee_slots_and_body(&cst, cr) {
                    if slots.len() >= 2 {
                        let slots_meta: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> = slots
                            .iter()
                            .map(|(n, sh, ty, _)| (n.as_str(), sh.clone(), ty.as_ref()))
                            .collect();
                        for (dst_idx, src_idx) in
                            self.collect_callee_param_stores(&slots_meta, &cbody, 0)
                        {
                            if let (Some((_, _, _, dst_src)), Some((_, _, _, src_src))) =
                                (slots.get(dst_idx), slots.get(src_idx))
                            {
                                let mut src_o = HashSet::new();
                                match src_src {
                                    CalleeSlotSource::Arg(src_e) => {
                                        self.collect_sharing_origins_expr(
                                            &cst, src_e, &origins, &shadowed, &mut src_o,
                                        );
                                    }
                                    CalleeSlotSource::OuterBinding(bname) => {
                                        if let Some(os) = origins.get(bname) {
                                            src_o.extend(os.iter().cloned());
                                        }
                                    }
                                }
                                let mut dst_roots = Vec::new();
                                match dst_src {
                                    CalleeSlotSource::Arg(dst_e) => {
                                        collect_ref_roots(dst_e, &mut dst_roots);
                                        collect_addr_of_roots(dst_e, &mut dst_roots);
                                    }
                                    CalleeSlotSource::OuterBinding(bname) => {
                                        dst_roots.push(bname.clone());
                                    }
                                }
                                for dr in dst_roots {
                                    if !src_o.is_empty() {
                                        origins
                                            .entry(dr)
                                            .or_default()
                                            .extend(src_o.iter().cloned());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            match stmt {
                Stmt::Local(l) => {
                    let is_let = matches!(l.kind, LocalKind::Let);
                    if l.values.is_empty() {
                        let shape = l
                            .ty
                            .as_ref()
                            .map(|t| self.shape_of_type(t))
                            .unwrap_or(TypeShape::Unknown);
                        for name in &l.names {
                            self.define_binding(
                                &mut cst,
                                name,
                                shape.clone(),
                                l.ty.clone(),
                                false,
                                false,
                                is_let,
                            );
                            if self.shape_may_share(&shape)
                                || matches!(shape, TypeShape::Unknown | TypeShape::Array(_))
                            {
                                origins.insert(
                                    name.clone(),
                                    HashSet::from([format!("local:{idx}:{name}")]),
                                );
                                shadowed.remove(name);
                            } else {
                                origins.remove(name);
                                shadowed.insert(name.clone());
                            }
                        }
                    } else if l.values.len() == 1 && l.names.len() > 1 {
                        let shapes = self.multi_value_shapes(&cst, &l.values[0], l.names.len());
                        let tys = self.multi_value_tys(&cst, &l.values[0], l.names.len());
                        let mut base_o = HashSet::new();
                        self.collect_sharing_origins_expr(
                            &cst,
                            &l.values[0],
                            &origins,
                            &shadowed,
                            &mut base_o,
                        );
                        for ((name, shape), ty) in l.names.iter().zip(shapes).zip(tys) {
                            self.define_binding(&mut cst, name, shape.clone(), ty, false, false, is_let);
                            if self.shape_may_share(&shape)
                                || matches!(shape, TypeShape::Unknown | TypeShape::Array(_))
                            {
                                let mut s = base_o.clone();
                                if s.is_empty() {
                                    s.insert(format!("local:{idx}:{name}"));
                                }
                                origins.insert(name.clone(), s);
                                shadowed.remove(name);
                            } else {
                                origins.remove(name);
                                shadowed.insert(name.clone());
                            }
                        }
                    } else {
                        let rhs_info: Vec<(
                            TypeShape,
                            Option<hy_syntax::TypeExpr>,
                            HashSet<String>,
                        )> = l
                            .values
                            .iter()
                            .map(|v| {
                                let inf = self.shape_of_expr(&cst, v);
                                let (sh, ty) = match (&l.ty, inf) {
                                    (Some(t), TypeShape::Unknown) => {
                                        (self.shape_of_type(t), Some(t.clone()))
                                    }
                                    (_, s) => (s, self.ty_of_expr(&cst, v).or_else(|| l.ty.clone())),
                                };
                                let mut os = HashSet::new();
                                self.collect_sharing_origins_expr(
                                    &cst, v, &origins, &shadowed, &mut os,
                                );
                                (sh, ty, os)
                            })
                            .collect();
                        for (name, (shape, ty, mut s)) in l.names.iter().zip(rhs_info) {
                            self.define_binding(&mut cst, name, shape.clone(), ty, false, false, is_let);
                            if self.shape_may_share(&shape)
                                || matches!(shape, TypeShape::Unknown | TypeShape::Array(_))
                            {
                                if s.is_empty() {
                                    s.insert(format!("local:{idx}:{name}"));
                                }
                                origins.insert(name.clone(), s);
                                shadowed.remove(name);
                            } else {
                                origins.remove(name);
                                shadowed.insert(name.clone());
                            }
                        }
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } => {
                    if targets.len() == values.len() {
                        let rhs_sets: Vec<(
                            TypeShape,
                            Option<hy_syntax::TypeExpr>,
                            HashSet<String>,
                        )> = values
                            .iter()
                            .map(|v| {
                                let sh = self.shape_of_expr(&cst, v);
                                let ty = self.ty_of_expr(&cst, v);
                                let mut os = HashSet::new();
                                self.collect_sharing_origins_expr(
                                    &cst, v, &origins, &shadowed, &mut os,
                                );
                                (sh, ty, os)
                            })
                            .collect();
                        for (t, (sh, ty, mut s)) in targets.iter().zip(rhs_sets) {
                            match &t.kind {
                                PlaceKind::Ident(name) => {
                                    if let Some(b) = cst.env_lookup_mut(name) {
                                        if !matches!(sh, TypeShape::Unknown) || b.ty.is_none() {
                                            b.shape = sh.clone();
                                        }
                                        if ty.is_some() || b.ty.is_none() {
                                            b.ty = ty;
                                        }
                                    }
                                    let target_shares = self
                                        .lookup_binding(&cst, name)
                                        .map(|b| {
                                            self.shape_may_share(&b.shape)
                                                || matches!(
                                                    b.shape,
                                                    TypeShape::Unknown | TypeShape::Array(_)
                                                )
                                        })
                                        .unwrap_or(true);
                                    if target_shares {
                                        if s.is_empty() {
                                            s.insert(format!("rebind:{idx}:{name}"));
                                        }
                                        origins.insert(name.clone(), s);
                                        shadowed.remove(name);
                                    } else {
                                        origins.remove(name);
                                        shadowed.insert(name.clone());
                                    }
                                }
                                PlaceKind::Field { .. } | PlaceKind::Index { .. } => {
                                    let root = t.root();
                                    if !root.is_empty() && !s.is_empty() {
                                        origins.entry(root.to_string()).or_default().extend(s);
                                    }
                                }
                                _ => {}
                            }
                        }
                    } else if values.len() == 1 && targets.len() > 1 {
                        let shapes = self.multi_value_shapes(&cst, &values[0], targets.len());
                        let mut base_o = HashSet::new();
                        self.collect_sharing_origins_expr(
                            &cst,
                            &values[0],
                            &origins,
                            &shadowed,
                            &mut base_o,
                        );
                        for (t, sh) in targets.iter().zip(shapes) {
                            if let PlaceKind::Ident(name) = &t.kind {
                                if self.shape_may_share(&sh)
                                    || matches!(sh, TypeShape::Unknown | TypeShape::Array(_))
                                {
                                    let mut s = base_o.clone();
                                    if s.is_empty() {
                                        s.insert(format!("rebind:{idx}:{name}"));
                                    }
                                    origins.insert(name.clone(), s);
                                    shadowed.remove(name);
                                } else {
                                    origins.remove(name);
                                    shadowed.insert(name.clone());
                                }
                            }
                        }
                    }
                }
                Stmt::Return { values, .. } => {
                    let sets: Vec<HashSet<String>> = if values.is_empty() {
                        if typed_results.len() < 2 {
                            continue;
                        }
                        typed_results
                            .iter()
                            .map(|(nr, _)| {
                                if nr.is_empty() || nr == "_" {
                                    HashSet::new()
                                } else {
                                    origins.get(nr).cloned().unwrap_or_default()
                                }
                            })
                            .collect()
                    } else {
                        if values.len() < 2 {
                            continue;
                        }
                        values
                            .iter()
                            .map(|v| {
                                if matches!(unwrap_parens(v), Expr::Move { .. }) {
                                    HashSet::new()
                                } else {
                                    let mut os = HashSet::new();
                                    self.collect_sharing_origins_expr(
                                        &cst, v, &origins, &shadowed, &mut os,
                                    );
                                    os
                                }
                            })
                            .collect()
                    };
                    for i in 0..sets.len() {
                        for j in (i + 1)..sets.len() {
                            if !sets[i].is_empty() && !sets[i].is_disjoint(&sets[j]) {
                                pairs.push((i, j));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }

    /// Connected component of `name` across both `may_share` and `aliases`
    /// (both parent and child directions), excluding `name` itself.
    pub(crate) fn storage_peers(&self, st: &FnState, name: &str) -> Vec<String> {
        let mut seen: HashSet<String> = HashSet::new();
        seen.insert(name.to_string());
        let mut stack = vec![name.to_string()];
        while let Some(x) = stack.pop() {
            if let Some(p) = st.aliases.get(&x) {
                if seen.insert(p.clone()) {
                    stack.push(p.clone());
                }
            }
            for (k, v) in &st.aliases {
                if v == &x && seen.insert(k.clone()) {
                    stack.push(k.clone());
                }
            }
            if let Some(peers) = st.may_share.get(&x) {
                for p in peers {
                    if seen.insert(p.clone()) {
                        stack.push(p.clone());
                    }
                }
            }
        }
        seen.remove(name);
        let mut out: Vec<String> = seen.into_iter().collect();
        out.sort();
        out
    }

    /// Before `name` is severed from `aliases` and `may_share` (by a shadowing
    /// local define or a non-self rebind), materialize `name`'s non-owned /
    /// `const` origin onto its surviving peers, preserve clique connectivity
    /// among those peers, record pre-shadow peers for `scope_pop` freeze
    /// propagation, and clone any live A2/A3 loans targeting `name` onto
    /// those surviving peers.
    pub(crate) fn preserve_peers_before_sever(
        &self,
        st: &mut FnState,
        name: &str,
        shadows_outer: bool,
    ) {
        let peers = self.storage_peers(st, name);
        if peers.is_empty() {
            return;
        }
        let mut inherited_shadows = st.shadow_links.get(name).cloned().unwrap_or_default();
        if shadows_outer {
            if let Some(depth) = st.lookup_depth(name) {
                inherited_shadows.insert((name.to_string(), depth));
            }
        }
        if !inherited_shadows.is_empty() {
            for peer in &peers {
                st.shadow_links
                    .entry(peer.clone())
                    .or_default()
                    .extend(inherited_shadows.iter().cloned());
            }
        }
        let direct_or_peer_origin = self
            .non_owned_origin_of(st, name)
            .or_else(|| {
                self.binding_const_root(st, name)
                    .map(|c| (format!("`const {}`", c), true))
            });
        if let Some((orig, via_share)) = direct_or_peer_origin {
            let peer_origin = if orig.starts_with("a may-share peer of ") {
                orig.clone()
            } else {
                format!("a may-share peer of {}", orig)
            };
            for peer in &peers {
                let is_direct_alias_child = st.aliases.get(peer).is_some_and(|v| v == name);
                let chosen = if is_direct_alias_child && !via_share {
                    &orig
                } else {
                    &peer_origin
                };
                if let Some(pb) = st.env_lookup_mut(peer) {
                    if pb.non_owned_origin.is_none() {
                        pb.non_owned_origin = Some(chosen.clone());
                    }
                }
            }
        }
        for idx_a in 0..peers.len() {
            for idx_b in (idx_a + 1)..peers.len() {
                st.share_edge(&peers[idx_a], &peers[idx_b]);
            }
        }
        let parent = st.aliases.get(name).cloned().filter(|p| p != name);
        let children: Vec<String> = st
            .aliases
            .iter()
            .filter_map(|(k, v)| if v == name && k != name { Some(k.clone()) } else { None })
            .collect();
        if let Some(p) = parent {
            for c in children {
                if c != p {
                    st.aliases.insert(c, p.clone());
                } else {
                    st.aliases.remove(&c);
                }
            }
        } else {
            let mut iter = children.into_iter();
            if let Some(first) = iter.next() {
                st.aliases.remove(&first);
                for c in iter {
                    st.aliases.insert(c, first.clone());
                }
            }
        }
        if !st.loans.is_empty() {
            let mut cloned_loans: Vec<Loan> = Vec::new();
            for loan in &st.loans {
                match &loan.target {
                    LoanTarget::Handle { handle } if a3_paths_conflict(st, handle, name) => {
                        for peer in &peers {
                            let exists = st.loans.iter().any(|l| {
                                l.created == loan.created
                                    && l.last == loan.last
                                    && l.span == loan.span
                                    && matches!(&l.target, LoanTarget::Handle { handle: h } if h == peer)
                            });
                            if !exists {
                                cloned_loans.push(Loan {
                                    target: LoanTarget::Handle {
                                        handle: peer.clone(),
                                    },
                                    created: loan.created,
                                    last: loan.last,
                                    span: loan.span,
                                    desc: loan.desc.clone(),
                                });
                            }
                        }
                    }
                    LoanTarget::MapSlot { binding, path, key }
                        if a3_paths_conflict(st, path, name) =>
                    {
                        for peer in &peers {
                            let exists = st.loans.iter().any(|l| {
                                l.created == loan.created
                                    && l.last == loan.last
                                    && l.span == loan.span
                                    && matches!(
                                        &l.target,
                                        LoanTarget::MapSlot { binding: b, path: p, .. }
                                            if b == binding && p == peer
                                    )
                            });
                            if !exists {
                                cloned_loans.push(Loan {
                                    target: LoanTarget::MapSlot {
                                        binding: binding.clone(),
                                        path: peer.clone(),
                                        key: key.clone(),
                                    },
                                    created: loan.created,
                                    last: loan.last,
                                    span: loan.span,
                                    desc: loan.desc.clone(),
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
            st.loans.extend(cloned_loans);
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
        is_let: bool,
    ) {
        if name == "_" {
            return;
        }
        let shadows_outer = st
            .scopes
            .last()
            .is_some_and(|top| !top.contains_key(name))
            && (st.env.contains_key(name)
                || st.scopes.iter().rev().skip(1).any(|s| s.contains_key(name))
                || self.consts.contains_key(name));
        self.preserve_peers_before_sever(st, name, shadows_outer);
        st.aliases.remove(name);
        st.share_clear(name);
        let non_owned_origin = if immutable {
            Some(format!("`const {}`", name))
        } else {
            None
        };
        st.env_define(
            name,
            Binding {
                shape,
                ty,
                immutable,
                is_let,
                managed_taint: managed, // O6.2: born inside `managed {}`
                moved: None,
                non_owned_origin,
                slot_loan: false,
            },
        );
    }

    /// Bind the payload variable `name` of a `match subject { some(name): ... }`
    /// arm in the arm's block scope (`#243`). `option[T]` lowers to
    /// `hyb_opt_T{v: T, ok: bool}`, so `some(name)` copies `subject.v` and
    /// shares mutable GC storage with `subject`, inherits its `non_owned_origin`
    /// and `moved` state, and links to any `const` root of `subject`.
    pub(crate) fn bind_match_arm(
        &mut self,
        st: &mut FnState,
        name: &str,
        subject: &Expr,
        managed: bool,
    ) {
        let shape = self.shape_of_expr(st, subject);
        let ty = match self.ty_of_expr(st, subject) {
            Some(hy_syntax::TypeExpr::Option { elem, .. }) => Some(*elem),
            other => other,
        };

        let mut roots = Vec::new();
        collect_ref_roots(subject, &mut roots);
        self.collect_callee_const_return_roots(st, subject, &mut roots, 0);

        let pre_tainted = roots.iter().any(|r| {
            self.lookup_binding(st, r)
                .map(|b| b.managed_taint)
                .unwrap_or(false)
        });
        let pre_moved = roots.iter().find_map(|r| {
            self.lookup_binding(st, r).and_then(|b| b.moved.clone())
        });
        let pre_origin = if self.shape_may_share(&shape) {
            roots.iter().find_map(|r| {
                self.non_owned_origin_of(st, r).map(|(o, via_share)| {
                    if via_share || (!shape.is_reference_alias() && r != name) {
                        format!("a may-share peer of {}", o)
                    } else {
                        o
                    }
                })
            })
        } else {
            None
        };
        let pre_const_root = roots.iter().find_map(|r| {
            let p = Place {
                kind: PlaceKind::Ident(r.clone()),
                span: subject.span(),
            };
            self.const_write_root(st, &p)
        });

        let mut pre_share_peers: Vec<String> = Vec::new();
        let mut pre_alias_root: Option<String> = None;
        for r in &roots {
            if self.binding_may_share(st, r)
                || self.expr_slices_addressable_array(st, subject, r)
                || !st.share_group(r).is_empty()
            {
                pre_share_peers.push(r.clone());
                pre_share_peers.extend(st.share_group(r));
            }
            if pre_alias_root.is_none()
                && self
                    .lookup_binding(st, r)
                    .map(|b| self.shape_may_share(&b.shape))
                    .unwrap_or(false)
            {
                pre_alias_root = Some(crate::freeze::resolve_alias(st, r));
            }
        }

        let self_shadows_const = pre_const_root.as_deref() == Some(name);
        self.define_binding(
            st,
            name,
            shape.clone(),
            ty,
            self_shadows_const,
            managed || pre_tainted,
            false,
        );

        self.record_may_share(st, name, subject);
        self.record_alias(st, name, subject);

        for peer in &pre_share_peers {
            if peer != name {
                st.share_edge(name, peer);
            }
        }
        if let Some(aroot) = pre_alias_root {
            st.aliases.entry(name.to_string()).or_insert(aroot);
        } else if pre_share_peers.iter().any(|p| p == name) {
            st.aliases
                .entry(name.to_string())
                .or_insert_with(|| name.to_string());
        }
        if let Some(croot) = pre_const_root {
            if croot != name {
                st.share_edge(name, &croot);
                st.aliases.entry(name.to_string()).or_insert(croot);
            }
        }

        if let Some(b) = st.env_lookup_mut(name) {
            if b.moved.is_none() {
                b.moved = pre_moved;
            }
            if b.non_owned_origin.is_none() {
                b.non_owned_origin = pre_origin;
            }
        }
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
        // A3 loan creation on assignment: `x = &m[k]`
        if op == AssignOp::Assign && targets.len() == values.len() {
            for (t, v) in targets.iter().zip(values.iter()) {
                self.check_addr_of_const_loan(st, v);
                if let PlaceKind::Ident(name) = &t.kind {
                    if let Some(path) = walk::addr_of_index_base_path(v) {
                        let binding = name.clone();
                        let key = walk::addr_of_index_slot_key(v);
                        self.emit_o8_alias_if_live(
                            st,
                            &path,
                            key.as_deref(),
                            Some(&binding),
                            v.span(),
                            idx,
                            "a second slot loan",
                        );
                        let last = st.last_use.get(&binding).copied().unwrap_or(idx).max(idx);
                        st.loans.push(Loan {
                            target: LoanTarget::MapSlot {
                                binding: binding.clone(),
                                path,
                                key,
                            },
                            created: idx,
                            last,
                            span: v.span(),
                            desc: format!("`{} = {}`", binding, v.render()),
                        });
                    } else {
                        self.extend_map_slot_loan_to_alias(st, name, v, idx);
                    }
                    if matches!(self.shape_of_expr(st, v), TypeShape::BorrowHandle(_)) {
                        self.extend_call_loans_to_view(st, v, name, idx);
                    }
                }
            }
        }
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
                    self.emit_const_write_error(&name, t.span);
                    true
                }
                None => {
                    if let PlaceKind::Ident(name) = &t.kind {
                        if let Some(b) = self.lookup_binding(st, name) {
                            if b.is_let {
                                self.emit_let_write_error(name, t.span);
                                return true;
                            }
                        }
                    }
                    false
                }
            })
            .collect();

        for t in targets {
            self.mark_move_in_place(st, t);
        }
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
            if let Some((path, key)) = walk::place_field_store_slot(t) {
                self.emit_o8_alias_if_live(
                    st,
                    &path,
                    key.as_deref(),
                    None,
                    t.span,
                    idx,
                    "a field store",
                );
            }
        }

        // A6: a borrow stored into a struct field (single, parallel, or multi-return)
        if targets.len() == values.len() {
            for (i, (t, v)) in targets.iter().zip(values.iter()).enumerate() {
                if is_const_write[i] {
                    continue;
                }
                if matches!(t.kind, PlaceKind::Field { .. }) {
                    let shape = self.shape_of_expr(st, v);
                    let is_borrow = as_addr_of(v).is_some()
                        || field_borrow_kind(&shape).is_some();
                    if is_borrow {
                        self.emit_a6_field_borrow_error(t);
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            for (t, shape) in targets.iter().zip(shapes.iter()) {
                if matches!(t.kind, PlaceKind::Field { .. })
                    && field_borrow_kind(shape).is_some()
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
            // Constant overflow on assignment: `n = 300`, `arr[0] = 300`, `n += 300`
            // (#998: the RHS may be a named file const, folded via const_values)
            if !matches!(op, AssignOp::ShlAssign | AssignOp::ShrAssign) {
                if let Some(target_ty) = self.place_target_ty(st, t) {
                    self.check_const_overflow_assign(&t.render(), &target_ty, v, Some(st));
                }
            }
            // Issue #146: `n = <-ch` where `n` has a known type and `ch` is
            // `chan T` with incompatible T.
            if let Some(want) = self.place_shape(st, t) {
                self.check_recv_elem_against_target(
                    st,
                    v,
                    &want,
                    v.span(),
                    &format!("receive into `{}`", t.render()),
                );
            }
        }

        // whole-binding rebind: `rows = append(rows, ...)`, parallel
        // `m2, x = m, 1`, and multi-value unpack `n, m = make_pair()`.
        if op == AssignOp::Assign {
            self.rebind_assign_targets(st, targets, values, &is_const_write, idx);
        }
    }

    pub(crate) fn process_closure_assign(
        &mut self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) {
        for t in targets {
            self.mark_move_in_place(st, t);
        }
        for v in values {
            self.mark_move_in_value(st, v, format!("the move-assign `{}`", v.render()));
        }
        if op == AssignOp::Assign {
            let no_const_mask = vec![false; targets.len()];
            self.rebind_assign_targets(st, targets, values, &no_const_mask, 0);
        }
        self.record_assign_borrow_aliases(st, targets, op, values);
    }

    fn rebind_assign_targets(
        &mut self,
        st: &mut FnState,
        targets: &[Place],
        values: &[Expr],
        is_const_write: &[bool],
        idx: usize,
    ) {
        if values.len() == targets.len() {
            let pre_st = st.clone();
            let mut overwritten = Vec::new();
            // Evaluate all RHS shapes, types, and alias sources before
            // mutating target bindings so parallel swaps/rebinds read the
            // pre-assignment environment.
            struct AssignRhsInfo {
                is_fresh_copy: bool,
                shape: TypeShape,
                ty: Option<hy_syntax::TypeExpr>,
                non_owned: Option<String>,
                reads_self: bool,
                shadowed_origin: Option<String>,
                prior_alias: Option<String>,
                prior_shares: Vec<String>,
                prior_shadows: HashSet<(String, usize)>,
            }
            let rhs_info: Vec<AssignRhsInfo> = targets
                .iter()
                .zip(values.iter())
                .map(|(t, v)| {
                    let shape = self.shape_of_expr(st, v);
                    let ty = self.ty_of_expr(st, v);
                    let non_owned = match crate::model::unwrap_parens(v) {
                        Expr::Ident { name: src, .. } => self
                            .lookup_binding(st, src)
                            .and_then(|b| b.non_owned_origin.clone()),
                        other => move_operand_ident(other)
                            .and_then(|src| self.lookup_binding(st, &src))
                            .and_then(|b| b.non_owned_origin.clone())
                            .or_else(|| self.moved_non_owned_origin_in_expr(st, other)),
                    };
                    let (reads_self, shadowed_origin, prior_alias, prior_shares, prior_shadows) =
                        match &t.kind {
                            PlaceKind::Ident(name) => {
                                let rs = self.value_reads_storage_of(st, v, name);
                                let so = if self.shape_may_share(&shape) && rs {
                                    self.non_owned_origin_of(st, name).map(|(o, via_share)| {
                                        if via_share && !o.starts_with("a may-share peer of ") {
                                            format!("a may-share peer of {}", o)
                                        } else {
                                            o
                                        }
                                    })
                                } else {
                                    None
                                };
                                let pa = if rs {
                                    st.aliases.get(name).cloned()
                                } else {
                                    None
                                };
                                let ps = if rs {
                                    st.may_share
                                        .get(name)
                                        .map(|s| s.iter().cloned().collect())
                                        .unwrap_or_default()
                                } else {
                                    Vec::new()
                                };
                                let psh = if rs {
                                    st.shadow_links.get(name).cloned().unwrap_or_default()
                                } else {
                                    HashSet::new()
                                };
                                (rs, so, pa, ps, psh)
                            }
                            _ => (false, None, None, Vec::new(), HashSet::new()),
                        };
                    AssignRhsInfo {
                        is_fresh_copy: self.call_returns_fresh_copy(st, v),
                        shape,
                        ty,
                        non_owned,
                        reads_self,
                        shadowed_origin,
                        prior_alias,
                        prior_shares,
                        prior_shadows,
                    }
                })
                .collect();

            for (i, ((t, v), info)) in targets
                .iter()
                .zip(values.iter())
                .zip(rhs_info)
                .enumerate()
            {
                if is_const_write.get(i).copied().unwrap_or(false) {
                    continue;
                }
                if let PlaceKind::Ident(name) = &t.kind {
                    if name == "_" {
                        continue;
                    }
                    overwritten.push(name.clone());
                    if !info.reads_self {
                        self.preserve_peers_before_sever(st, name, false);
                    }

                    if let Some(b) = st.env_lookup_mut(name) {
                        if !matches!(info.shape, TypeShape::Unknown) || b.ty.is_none() {
                            b.shape = info.shape;
                        }
                        if info.ty.is_some()
                            || b.ty.is_none()
                            || matches!(
                                b.ty.as_ref().map(|t| self.resolve_type_alias(t)),
                                Some(hy_syntax::TypeExpr::Fn { .. })
                            )
                        {
                            b.ty = info.ty;
                        }
                        b.moved = None; // a fresh value re-owns the binding
                        b.non_owned_origin = info.non_owned;
                    }
                    // a rebind re-owns the binding: drop the old alias and
                    // may-share edges, then re-record from the new value (#27)
                    st.aliases.remove(name);
                    st.share_clear(name);
                    if info.reads_self {
                        if !info.prior_shadows.is_empty() {
                            st.shadow_links
                                .entry(name.clone())
                                .or_default()
                                .extend(info.prior_shadows);
                        }
                        if let Some(pa) = info.prior_alias {
                            if &pa != name {
                                st.aliases.insert(name.clone(), pa);
                            }
                        }
                        for peer in info.prior_shares {
                            st.share_edge(name, &peer);
                        }
                    }
                    if !info.is_fresh_copy { self.record_storage_from_snapshot(st, &pre_st, name, v, &overwritten, false, false); }
                    self.record_move_const_and_a3_loans(st, name, v, idx);
                    // rebind may create a fresh alias — ident copies AND
                    // call results that hand back argument storage (#38)

                    if let Some(origin) = info.shadowed_origin {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                    }
                    // #111: a closure rebind replaces the old body's
                    // mutation set (or clears it for non-closure values)
                    st.closure_mutates.remove(name);
                    st.closure_defs.remove(name);
                    st.closure_fnlits.remove(name);
                    self.record_closure_mutations(st, name, v);
                } else {
                    let root = t.root();
                    if !root.is_empty() && self.binding_may_share(st, root) {
                        self.record_may_share(st, root, v);
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            let moved_non_owned = self.moved_non_owned_origin_in_expr(st, &values[0]);
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            let tys = self.multi_value_tys(st, &values[0], targets.len());
            let mut ref_roots = Vec::new();
            collect_ref_roots(&values[0], &mut ref_roots);
            self.collect_callee_const_return_roots(st, &values[0], &mut ref_roots, 0);
            for (i, ((t, shape), ty)) in targets.iter().zip(shapes).zip(tys).enumerate() {
                if is_const_write.get(i).copied().unwrap_or(false) {
                    continue;
                }
                if let PlaceKind::Ident(name) = &t.kind {
                    let may_share = self.shape_may_share(&shape);
                    let reads_self = may_share
                        && (self.value_reads_storage_of(st, &values[0], name)
                            || ref_roots.iter().any(|r| r == name));
                    let prior_alias = if reads_self {
                        st.aliases.get(name).cloned()
                    } else {
                        None
                    };
                    let prior_children: Vec<String> = if reads_self {
                        st.aliases
                            .iter()
                            .filter_map(|(k, v)| {
                                if v == name && k != name {
                                    Some(k.clone())
                                } else {
                                    None
                                }
                            })
                            .collect()
                    } else {
                        Vec::new()
                    };
                    let prior_shares: Vec<String> = if reads_self {
                        st.may_share
                            .get(name)
                            .map(|s| s.iter().cloned().collect())
                            .unwrap_or_default()
                    } else {
                        Vec::new()
                    };
                    let prior_shadows = if reads_self {
                        st.shadow_links.get(name).cloned().unwrap_or_default()
                    } else {
                        HashSet::new()
                    };
                    let shadowed_origin = if reads_self {
                        self.non_owned_origin_of(st, name).map(|(o, via_share)| {
                            if via_share && !o.starts_with("a may-share peer of ") {
                                format!("a may-share peer of {}", o)
                            } else {
                                o
                            }
                        })
                    } else {
                        None
                    };
                    if !reads_self {
                        self.preserve_peers_before_sever(st, name, false);
                    }
                    if let Some(b) = st.env_lookup_mut(name) {
                        if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                            b.shape = shape.clone();
                        }
                        if ty.is_some() || b.ty.is_none() {
                            b.ty = ty;
                        }
                        b.moved = None;
                        b.non_owned_origin = if may_share {
                            moved_non_owned.clone()
                        } else {
                            None
                        };
                    }
                    st.aliases.remove(name);
                    st.share_clear(name);
                    if reads_self {
                        if !prior_shadows.is_empty() {
                            st.shadow_links
                                .entry(name.clone())
                                .or_default()
                                .extend(prior_shadows);
                        }
                        if let Some(pa) = prior_alias {
                            if &pa != name {
                                st.aliases.insert(name.clone(), pa);
                            }
                        }
                        for child in prior_children {
                            if &child != name {
                                st.aliases.insert(child, name.clone());
                            }
                        }
                        for peer in prior_shares {
                            st.share_edge(name, &peer);
                        }
                    }
                    self.record_multi_value_share_and_alias(st, name, &shape, &values[0]);
                    if may_share {
                        for r in &ref_roots {
                            if r != name && self.binding_may_share(st, r) {
                                st.share_edge(name, r);
                                if !st.aliases.contains_key(name) {
                                    st.aliases.insert(name.clone(), r.clone());
                                }
                                if let Some(orig) = self
                                    .lookup_binding(st, r)
                                    .and_then(|b| b.non_owned_origin.clone())
                                {
                                    if let Some(nb) = st.env_lookup_mut(name) {
                                        if nb.non_owned_origin.is_none() {
                                            nb.non_owned_origin = Some(orig);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    if let Some(origin) = shadowed_origin {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                    }
                    if may_share {
                        self.record_move_const_and_a3_loans(st, name, &values[0], idx);
                    }
                } else {
                    let root = t.root();
                    if !root.is_empty() && self.binding_may_share(st, root) {
                        self.record_multi_value_may_share(st, root, &shape, &values[0]);
                    }
                }
            }
            // Every target is rebound now: join results the callee returns twice.
            let bound: Vec<String> = targets
                .iter()
                .map(|t| match &t.kind {
                    PlaceKind::Ident(n) => n.clone(),
                    _ => t.root().to_string(),
                })
                .collect();
            self.link_multi_value_return_aliases(st, &values[0], &bound);
        }
    }



    pub(crate) fn emit_a6_field_borrow_error(&mut self, t: &Place) {
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
    fn binding_const_root(&self, st: &FnState, name: &str) -> Option<String> {
        if let Some(b) = self.lookup_binding(st, name) {
            if b.immutable {
                return Some(name.to_string());
            }
            if let Some(origin) = &b.non_owned_origin {
                if let Some(idx) = origin.find("`const ") {
                    let rest = &origin[idx + 7..];
                    if let Some(end) = rest.find('`') {
                        let const_name = &rest[..end];
                        // A copy of a scalar (or otherwise immutable-by-value) const is a
                        // value copy, not a write-through alias: `const k = 3; x = k; x = 1`
                        // must stay writable. Only consts that can share backing storage
                        // (#123: map/slice/pointer/handle/struct-or-array containing them)
                        // make a write through the copy a write to the const.
                        if let Some(cb) = self.lookup_binding(st, const_name) {
                            if matches!(
                                cb.shape,
                                TypeShape::Primitive | TypeShape::Str | TypeShape::Atomic
                            ) {
                                return None;
                            }
                        }
                        return Some(const_name.to_string());
                    }
                }
            }
        }
        None
    }

    /// Resolve a write target to its `const` root. Fn-local and file-level
    /// all resolve through here (T22 / issue #37).
    fn const_write_root(&self, st: &FnState, p: &Place) -> Option<String> {
        match &p.kind {
            PlaceKind::Ident(name) => {
                if name == "_" {
                    return None;
                }
                if let Some(hit) = self.binding_const_root(st, name) {
                    return Some(hit);
                }
                // #123: a copy of a `const` (`alias := cells`, `let copy =
                // cells`) points at the same backing storage, so writing
                // through the copy IS a write to the const. Follow alias
                // parents and may-share peers to the immutable root.
                let mut seen: HashSet<String> = HashSet::new();
                let mut stack = vec![name.clone()];
                while let Some(n) = stack.pop() {
                    if !seen.insert(n.clone()) {
                        continue;
                    }
                    if let Some(parent) = st.aliases.get(&n) {
                        if let Some(hit) = self.binding_const_root(st, parent) {
                            return Some(hit);
                        }
                        stack.push(parent.clone());
                    }
                    for peer in st.share_group(&n) {
                        if seen.contains(&peer) {
                            continue;
                        }
                        if let Some(hit) = self.binding_const_root(st, &peer) {
                            return Some(hit);
                        }
                        stack.push(peer);
                    }
                }
                None
            }
            PlaceKind::Field { base, .. } | PlaceKind::Index { base, .. } => {
                self.const_write_root(st, base)
            }
            _ => None,
        }
    }

    /// O8 slot/field loans (`&m[k]`, `&c.field`) lower in `hy-emit` to a copy
    /// plus an unconditional store-back (`m[k] = s`, `c.field = f`) at the end
    /// of the declaring block. Taking an index or field loan of a `const`
    /// therefore writes to the `const` at runtime (T22, HYB-E3006).
    pub(crate) fn check_addr_of_const_loan(&mut self, st: &FnState, val: &Expr) {
        let Some(operand) = as_addr_of(val) else {
            return;
        };
        let operand = unwrap_parens(operand);
        if !matches!(operand, Expr::Index { .. } | Expr::Field { .. }) {
            return;
        }
        let mut roots = Vec::new();
        if let Some(root) = operand.root_ident() {
            roots.push(root.to_string());
        } else if !self.addr_operand_returns_fresh_copy(st, operand) {
            collect_ref_roots(operand, &mut roots);
            self.collect_callee_const_return_roots(st, operand, &mut roots, 0);
        }
        for root in roots {
            let place = Place {
                kind: PlaceKind::Ident(root),
                span: val.span(),
            };
            if let Some(const_name) = self.const_write_root(st, &place) {
                self.emit_const_write_error(&const_name, val.span());
                break;
            }
        }
    }

    /// HYB-E3006 — the shared T22 deep-immutability diagnostic (issue #37 /
    /// #123): the write site's span, the resolved const in the message.
    fn emit_const_write_error(&mut self, name: &str, span: Span) {
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
            Diagnostic::error("HYB-E3006", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// HYB-E3006 — T22 binding-level immutability diagnostic (#655):
    /// `let x = e` creates an immutable binding (no rebinding/reassignment).
    fn emit_let_write_error(&mut self, name: &str, span: Span) {
        let msg = format!(
            "cannot reassign to `{}` — it is a `let` binding (immutable)",
            name
        );
        let notes = vec![
            "`let` creates an immutable binding (SPEC-LANGUAGE T22): no rebinding or reassignment".to_string(),
            "interior mutation through an alias is possible, but the binding itself cannot be reassigned".to_string(),
            "use `var` or `:=` if the binding needs to be reassigned".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3006", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// #123: closure bodies sit outside Pass B (the A5 perimeter — flatten,
    /// uses, and calls never descend into an `FnLit`), so `process_assign`
    /// never sees a write to a `const` inside a closure. For every closure
    /// reachable from the function body (any expression position), scan its
    /// OWN statements for assign targets and `delete()` calls whose root
    /// resolves to an immutable binding, with the completed fn env (file
    /// consts resolve through `lookup_binding`; fn-local consts are still
    /// in scope). The function's top-level statements are process_assign /
    /// process_call's business — they are NOT rescanned here (no duplicate
    /// diagnostics).
    pub(crate) fn check_closure_const_writes(&mut self, st: &FnState, body: &Block) {
        self.scan_block_const_writes(st, body, false);
    }

    fn scan_block_const_writes(&mut self, st: &FnState, body: &Block, check_writes: bool) {
        let mut cst = st.clone();
        let flat = walk::flatten_with_results(body, &cst.named_results);
        cst.last_use = flat.last_use.clone();
        for (stmt, idx) in &flat.stmts {
            let idx = *idx;
            if let Some(evs) = flat.events.get(&idx) {
                for ev in evs {
                    match ev {
                        walk::ScopeEvent::Push => cst.scope_push(),
                        walk::ScopeEvent::Pop => cst.scope_pop(),
                        walk::ScopeEvent::SelectRecv { names, chan } => {
                            let elem = match self.shape_of_expr(&cst, chan) {
                                TypeShape::Chan(e) => *e,
                                _ => TypeShape::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                let shape = if i == 0 {
                                    elem.clone()
                                } else {
                                    TypeShape::Primitive
                                };
                                self.define_binding(&mut cst, n, shape, None, false, false, false);
                            }
                        }
                        walk::ScopeEvent::MatchBind { name, subject } => {
                            // pr-244 (#243): match-arm `some(v)` bindings
                            // inherit the subject's const-root state — a
                            // binding over a const composite freezes like the
                            // const itself (E3006 surface); a plain binding
                            // shadows and clears alias/share state.
                            let mut roots = Vec::new();
                            collect_ref_roots(subject, &mut roots);
                            self.collect_callee_const_return_roots(&cst, subject, &mut roots, 0);
                            let croot = roots.iter().find_map(|r| {
                                if self.lookup_binding(&cst, r).is_none() {
                                    return None;
                                }
                                let p = Place {
                                    kind: PlaceKind::Ident(r.clone()),
                                    span: subject.span(),
                                };
                                self.const_write_root(&cst, &p)
                            });
                            if let Some(cr) = &croot {
                                if name == cr {
                                    self.define_binding(
                                        &mut cst,
                                        name,
                                        TypeShape::Unknown,
                                        None,
                                        true,
                                        false,
                                        false,
                                    );
                                } else {
                                    cst.aliases.insert(name.to_string(), cr.clone());
                                    cst.share_edge(name, cr);
                                }
                            } else {
                                self.define_binding(
                                    &mut cst,
                                    name,
                                    TypeShape::Unknown,
                                    None,
                                    false,
                                    false,
                                    false,
                                );
                                cst.aliases.remove(*name);
                                cst.share_clear(name);
                            }
                        }
                    }
                }
            }

            if check_writes {
                let mut calls = Vec::new();
                if let Stmt::For {
                    kind: hy_syntax::ForKind::Iterate { expr, .. },
                    ..
                } = stmt
                {
                    walk::expr_calls(expr, &mut calls);
                } else {
                    walk::own_calls(stmt, &mut calls);
                }
                for cr in &calls {
                    self.check_delete_const(&cst, cr, &HashSet::new());
                    self.check_call_const_writes(&cst, cr, &HashSet::new());
                    if let Expr::Ident { name, .. } = crate::model::unwrap_parens(cr.callee) {
                        if name == "copy" {
                            self.record_copy_may_share(&mut cst, cr);
                        }
                    }
                    self.record_callee_param_stores(&mut cst, cr);
                }
                if let Stmt::Task { callee, args, span } = stmt {
                    let cr = CallRef {
                        callee,
                        args,
                        span: *span,
                    };
                    self.check_call_const_writes(&cst, &cr, &HashSet::new());
                }
            }

            match stmt {
                Stmt::Local(l) => {
                    if check_writes {
                        for v in &l.values {
                            self.check_addr_of_const_loan(&cst, v);
                        }
                    }
                    self.process_closure_const_local(&mut cst, l);
                    self.record_local_borrow_aliases(&mut cst, l);
                }
                Stmt::Assign {
                    targets,
                    op,
                    values,
                    ..
                } => {
                    if check_writes {
                        for v in values {
                            self.check_addr_of_const_loan(&cst, v);
                        }
                    }
                    let is_const_write: Vec<bool> = targets
                        .iter()
                        .map(|t| match self.const_write_root(&cst, t) {
                            Some(name) => {
                                if check_writes {
                                    self.emit_const_write_error(&name, t.span);
                                }
                                true
                            }
                            None => {
                                if let PlaceKind::Ident(name) = &t.kind {
                                    if let Some(b) = self.lookup_binding(&cst, name) {
                                        if b.is_let {
                                            if check_writes {
                                                self.emit_let_write_error(name, t.span);
                                            }
                                            return true;
                                        }
                                    }
                                }
                                false
                            }
                        })
                        .collect();

                    if *op == AssignOp::Assign && targets.len() == values.len() {
                        let rhs_info: Vec<(TypeShape, Option<hy_syntax::TypeExpr>)> = values
                            .iter()
                            .map(|v| (self.shape_of_expr(&cst, v), self.ty_of_expr(&cst, v)))
                            .collect();
                        for (i, ((t, v), (shape, ty))) in targets
                            .iter()
                            .zip(values.iter())
                            .zip(rhs_info)
                            .enumerate()
                        {
                            if is_const_write[i] {
                                continue;
                            }
                            if let PlaceKind::Ident(name) = &t.kind {
                                if name == "_" {
                                    continue;
                                }
                                if !self.value_reads_storage_of(&cst, v, name) {
                                    self.preserve_peers_before_sever(&mut cst, name, false);
                                }
                                if let Some(b) = cst.env_lookup_mut(name) {
                                    if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                                        b.shape = shape;
                                    }
                                    if ty.is_some() || b.ty.is_none() {
                                        b.ty = ty;
                                    }
                                }
                                cst.aliases.remove(name);
                                cst.share_clear(name);
                                self.record_may_share(&mut cst, name, v);
                                self.record_move_const_and_a3_loans(&mut cst, name, v, 0);
                                self.record_alias(&mut cst, name, v);
                                cst.closure_mutates.remove(name);
                                cst.closure_defs.remove(name);
                                cst.closure_fnlits.remove(name);
                                self.record_closure_mutations(&mut cst, name, v);
                            } else {
                                let root = t.root();
                                if !root.is_empty() && self.binding_may_share(&cst, root) {
                                    self.record_may_share(&mut cst, root, v);
                                }
                            }
                        }
                        self.record_assign_borrow_aliases(&mut cst, targets, *op, values);
                    } else if *op == AssignOp::Assign && values.len() == 1 && targets.len() > 1 {
                        let shapes = self.multi_value_shapes(&cst, &values[0], targets.len());
                        let tys = self.multi_value_tys(&cst, &values[0], targets.len());
                        for (i, ((t, shape), ty)) in
                            targets.iter().zip(shapes).zip(tys).enumerate()
                        {
                            if is_const_write.get(i).copied().unwrap_or(false) {
                                continue;
                            }
                            if let PlaceKind::Ident(name) = &t.kind {
                                if let Some(b) = cst.env_lookup_mut(name) {
                                    if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                                        b.shape = shape.clone();
                                    }
                                    if ty.is_some() || b.ty.is_none() {
                                        b.ty = ty;
                                    }
                                }
                                cst.aliases.remove(name);
                                cst.share_clear(name);
                                self.record_multi_value_share_and_alias(
                                    &mut cst,
                                    name,
                                    &shape,
                                    &values[0],
                                );
                            } else {
                                let root = t.root();
                                if !root.is_empty() && self.binding_may_share(&cst, root) {
                                    self.record_multi_value_may_share(
                                        &mut cst,
                                        root,
                                        &shape,
                                        &values[0],
                                    );
                                }
                            }
                        }
                        let bound: Vec<String> = targets
                            .iter()
                            .map(|t| match &t.kind {
                                PlaceKind::Ident(n) => n.clone(),
                                _ => t.root().to_string(),
                            })
                            .collect();
                        self.link_multi_value_return_aliases(&mut cst, &values[0], &bound);
                        self.record_assign_borrow_aliases(&mut cst, targets, *op, values);
                    }
                }
                Stmt::For { kind, .. } => match kind {
                    hy_syntax::ForKind::Iterate { vars, expr } => {
                        let is_chan =
                            matches!(self.shape_of_expr(&cst, expr), TypeShape::Chan(_));
                        let shapes = self.iterate_var_shapes(&cst, expr, vars.len());
                        for (name, shape) in vars.iter().zip(shapes) {
                            let is_ref = shape.is_reference_alias() || shape.is_handle();
                            let carries = self.shape_may_share(&shape);
                            self.define_binding(&mut cst, name, shape, None, false, false, false);
                            if !is_chan {
                                let mut roots = Vec::new();
                                collect_ref_roots(expr, &mut roots);
                                self.collect_callee_const_return_roots(&cst, expr, &mut roots, 0);
                                for root in roots {
                                    if self.lookup_binding(&cst, &root).is_some() {
                                        if is_ref {
                                            cst.aliases.insert(name.clone(), root.clone());
                                        }
                                        if carries && self.binding_may_share(&cst, &root) {
                                            cst.share_edge(name, &root);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    hy_syntax::ForKind::Range { var, .. } => {
                        self.define_binding(&mut cst, var, TypeShape::Primitive, None, false, false, false);
                    }
                    _ => {}
                },
                _ => {}
            }

            let mut fnlits = Vec::new();
            walk::own_fnlits(stmt, &mut fnlits);
            for flit in fnlits {
                let mut merged = cst.clone();
                // Also preserve any top-level alias/share edges from the completed
                // enclosing state `st` for bindings that are not shadowed in an
                // inner scope of `merged` (covers post-closure rebinds to a const).
                for (k, v) in &st.aliases {
                    if !merged.scopes.iter().any(|s| s.contains_key(k)) {
                        merged.aliases.entry(k.clone()).or_insert_with(|| v.clone());
                    }
                }
                for (k, peers) in &st.may_share {
                    if !merged.scopes.iter().any(|s| s.contains_key(k)) {
                        for p in peers {
                            if !merged.scopes.iter().any(|s| s.contains_key(p)) {
                                merged.share_edge(k, p);
                            }
                        }
                    }
                }
                self.scan_closure_body(&mut merged, flit, &HashSet::new());
            }
        }
    }

    /// Scan one closure's body with its own lexical scope and parameters,
    /// then recurse into every nested closure found in its statements.
    fn scan_closure_body(&mut self, st: &mut FnState, flit: &Expr, _declared: &HashSet<String>) {
        let Expr::FnLit {
            params,
            results,
            body,
            ..
        } = crate::model::unwrap_parens(flit)
        else {
            return;
        };
        // pr-213 architecture: params live in their own scope and SHADOW
        // outer names; the body is scanned by the SAME ordered single-pass
        // walker the driver uses (locals define in order, assigns check
        // const roots and record rebind edges, scope events nest) — a
        // two-pass shape cannot model pre-shadow order (the
        // closure_pre_shadow pin) or rebind-then-write (the
        // closure_local_alias pin).
        st.scope_push();
        for p in params {
            let shape = self.shape_of_type(&p.ty);
            self.define_binding(st, &p.name, shape, Some(p.ty.clone()), false, false, false);
        }
        for r in results {
            if let Some(name) = &r.name {
                if name != "_" {
                    let shape = self.shape_of_type(&r.ty);
                    self.define_binding(st, name, shape, Some(r.ty.clone()), false, false, false);
                }
            }
        }
        self.scan_block_const_writes(st, body, true);
        st.scope_pop();
    }

    /// pr-213 (#212): inside closures, a `may_share`-shaped local whose
    /// initializer reads the storage of a `const` root is itself frozen
    /// (E3006 surface) — the same-stmt share-move form of a const write.
    fn process_closure_const_local(&self, st: &mut FnState, l: &LocalDecl) {
        let immutable = l.kind == LocalKind::Const;
        let pre_st = st.clone();
        let mut overwritten = Vec::new();
        let is_let = l.kind == LocalKind::Let;
        if l.values.len() == 1 && l.names.len() > 1 {
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            let tys = self.multi_value_tys(st, &l.values[0], l.names.len());
            let resolved: Vec<(String, TypeShape, Option<hy_syntax::TypeExpr>, bool)> = l
                .names
                .iter()
                .zip(shapes)
                .zip(tys)
                .map(|((name, shape), ty)| {
                    let (shape, ty) = match (&l.ty, shape) {
                        (Some(t), TypeShape::Unknown) => {
                            (self.shape_of_type(t), Some(t.clone()))
                        }
                        (_, s) => (s, ty.or_else(|| l.ty.clone())),
                    };
                    let shadowed_const = self.shape_may_share(&shape)
                        && self.value_reads_storage_of(st, &l.values[0], name)
                        && self
                            .const_write_root(
                                st,
                                &Place {
                                    kind: PlaceKind::Ident(name.clone()),
                                    span: l.values[0].span(),
                                },
                            )
                            .is_some();
                    (name.clone(), shape, ty, immutable || shadowed_const)
                })
                .collect();
            for (name, shape, ty, is_imm) in resolved {
                overwritten.push(name.clone());
                self.define_binding(st, &name, shape.clone(), ty, is_imm, false, is_let);
                if move_operand_ident(&l.values[0]).is_none() {
                    self.record_storage_from_snapshot(st, &pre_st, &name, &l.values[0], &overwritten, true, true);
                }
            }
            self.link_multi_value_return_aliases(st, &l.values[0], &l.names);
            return;
        }
        let pre_st = st.clone();
        let mut overwritten = Vec::new();
        for (name, value) in l.names.iter().zip(l.values.iter()) {
            overwritten.push(name.clone());
            let inferred = self.shape_of_expr(&pre_st, value);
            let (shape, ty) = match (&l.ty, inferred) {
                (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                (_, inferred) => (
                    inferred,
                    l.ty.clone().or_else(|| self.ty_of_expr(&pre_st, value)),
                ),
            };
            let is_fresh_copy = self.call_returns_fresh_copy(&pre_st, value);
                let shadowed_const = !is_fresh_copy && self.shape_may_share(&shape)
                && self.value_reads_storage_of(&pre_st, value, name)
                && self
                    .const_write_root(
                        &pre_st,
                        &Place {
                            kind: PlaceKind::Ident(name.clone()),
                            span: value.span(),
                        },
                    )
                    .is_some();
            self.define_binding(st, name, shape, ty, immutable || shadowed_const, false, is_let);
            if move_operand_ident(value).is_none() && !is_fresh_copy {
                self.record_storage_from_snapshot(st, &pre_st, name, value, &overwritten, true, false);
            }
            self.record_closure_mutations(st, name, value);
            self.record_move_const_and_a3_loans(st, name, value, 0);
        }
    }

    fn check_delete_const(&mut self, st: &FnState, cr: &CallRef, declared: &HashSet<String>) {
        let Expr::Ident { name, .. } = crate::model::unwrap_parens(cr.callee) else {
            return;
        };
        if !matches!(name.as_str(), "delete" | "copy" | "clear" | "append") {
            return;
        }
        let Some(first) = cr.args.first() else {
            return;
        };
        if name.as_str() == "append" && self.append_dst_reallocates(st, &first.value) {
            return;
        }
        if let Some(hit) = self.const_arg_root(st, &first.value, declared) {
            self.emit_const_write_error(&hit, first.value.span());
        }
    }

    /// `append(dst, ..)` on a `const` writes nothing back into the `const`
    /// when `dst` has no spare capacity. A `const` composite is
    /// literal-initialized (`cap == len`), so the `const` itself, a field or
    /// element of it, or an open-ended tail `c[i:]` always reallocates. A
    /// bounded reslice `c[:k]` / `c[a:b]`, or a local alias of one, may not.
    fn append_dst_reallocates(&self, st: &FnState, dst: &Expr) -> bool {
        let mut cur = crate::model::unwrap_parens(dst);
        loop {
            match cur {
                Expr::Ident { name, .. } => {
                    return self.lookup_binding(st, name).is_some_and(|b| b.immutable);
                }
                Expr::Field { base, .. } | Expr::Index { base, .. } => {
                    cur = crate::model::unwrap_parens(base);
                }
                Expr::Slice { base, hi: None, .. } => cur = crate::model::unwrap_parens(base),
                _ => return false,
            }
        }
    }

    /// Resolve a call argument expression to a `const` root when the argument
    /// passes mutable reference storage (map, slice, pointer, `&const_val`, or
    /// a struct/array carrying reference fields) backed by that `const` (#188).
    fn const_arg_root(
        &self,
        st: &FnState,
        expr: &Expr,
        declared: &HashSet<String>,
    ) -> Option<String> {
        let inner = crate::model::unwrap_parens(expr);
        if let Some(addr_op) = as_addr_of(inner) {
            let mut roots = Vec::new();
            if let Some(root) = addr_op.root_ident() {
                roots.push(root.to_string());
            } else if !self.addr_operand_returns_fresh_copy(st, addr_op) {
                collect_ref_roots(addr_op, &mut roots);
                self.collect_callee_const_return_roots(st, addr_op, &mut roots, 0);
            }
            for root in roots {
                if declared.contains(&root) {
                    continue;
                }
                let place = Place {
                    kind: PlaceKind::Ident(root),
                    span: expr.span(),
                };
                if let Some(hit) = self.const_write_root(st, &place) {
                    return Some(hit);
                }
            }
            return None;
        }
        let arg_shape = self.shape_of_expr(st, inner);
        if !(self.shape_may_share(&arg_shape) || matches!(arg_shape, TypeShape::Unknown)) {
            return None;
        }
        let mut roots = Vec::new();
        collect_ref_roots(inner, &mut roots);
        self.collect_callee_const_return_roots(st, inner, &mut roots, 0);
        collect_move_ident_sources(inner, &mut roots);
        if roots.is_empty() {
            if let Some(r) = inner.root_ident() {
                roots.push(r.to_string());
            }
        }
        let mut move_ops = Vec::new();
        crate::model::collect_move_operands(inner, &mut move_ops);
        for (op, _) in move_ops {
            let root = match op {
                Expr::Ident { name, .. } => Some(name.as_str()),
                Expr::Field { .. } => op.root_ident(),
                _ => None,
            };
            if let Some(r) = root {
                roots.push(r.to_string());
            }
        }
        for root in roots {
            if declared.contains(&root) {
                continue;
            }
            let place = Place {
                kind: PlaceKind::Ident(root),
                span: expr.span(),
            };
            if let Some(hit) = self.const_write_root(st, &place) {
                return Some(hit);
            }
        }
        None
    }

    /// Issue #188: interprocedural T22 enforcement (`HYB-E3006`). When a
    /// `const` composite with reference storage (or `&const_val`) is passed to
    /// a function or closure that mutates that parameter (via index/field
    /// store, `delete`, `copy`, channel send, task spawn, or transitive call),
    /// reject the call site with `HYB-E3006` (`cannot write to const`).
    /// Read-only helpers (`len(shared_map)`, `count(shared_map)`) do not
    /// mutate the parameter and remain accepted.
    pub(crate) fn check_call_const_writes(&mut self, st: &FnState, cr: &CallRef, declared: &HashSet<String>) {
        let callee = crate::model::unwrap_parens(cr.callee);
        if let Expr::Ident { name, .. } = callee {
            if name == "delete"
                || name == "copy"
                || name == "clear"
                || A3_PURE_FNS.contains(&name.as_str())
            {
                return;
            }
        }
        let mut hits: Vec<(String, Span)> = Vec::new();
        if let Expr::Field {
            base,
            name: method_name,
            ..
        } = callee
        {
            let recv_base = match self.shape_of_expr(st, base) {
                TypeShape::Struct(Some(s)) => Some(s),
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Struct(Some(s)) => Some(s),
                    _ => None,
                },
                _ => None,
            };
            if let Some(rname) = recv_base {
                if let Some(decl) = self
                    .method_decls
                    .get(&(rname, method_name.clone()))
                    .copied()
                {
                    if let Some(r) = &decl.receiver {
                        let ptr_recv = matches!(r.ty, hy_syntax::TypeExpr::Pointer { .. });
                        let recv_const = if ptr_recv {
                            base.root_ident()
                                .filter(|root| !declared.contains(*root))
                                .and_then(|root| {
                                    self.const_write_root(
                                        st,
                                        &Place {
                                            kind: PlaceKind::Ident(root.to_string()),
                                            span: base.span(),
                                        },
                                    )
                                })
                        } else {
                            self.const_arg_root(st, base, declared)
                        };
                        if let Some(const_name) = recv_const {
                            if self.fn_mutates_const_param(decl, &r.name, 0) {
                                hits.push((const_name, base.span()));
                            }
                        }
                    }
                }
            } else if method_name == "pop"
                && cr.args.is_empty()
                && matches!(
                    self.shape_of_expr(st, base),
                    TypeShape::Slice(_) | TypeShape::Unknown
                )
            {
                let recv_const = base
                    .root_ident()
                    .filter(|root| !declared.contains(*root))
                    .and_then(|root| {
                        self.const_write_root(
                            st,
                            &Place {
                                kind: PlaceKind::Ident(root.to_string()),
                                span: base.span(),
                            },
                        )
                    })
                    .or_else(|| self.const_arg_root(st, base, declared));
                if let Some(const_name) = recv_const {
                    hits.push((const_name, base.span()));
                }
            }
        }
        for (i, a) in cr.args.iter().enumerate() {
            let inner = crate::model::unwrap_parens(&a.value);
            let is_addressable = matches!(
                inner,
                Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. }
            );
            let already_ptr = matches!(self.shape_of_expr(st, &a.value), TypeShape::Pointer(_))
                || as_addr_of(inner).is_some();

            let (param_is_ptr, mutates) = match callee {
                Expr::Ident { name, .. } => {
                    if declared.contains(name) {
                        (false, true)
                    } else if let Some(decl) = self.fns.get(name) {
                        match decl.params.get(i) {
                            Some(p) => {
                                let is_ptr = matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. })
                                    || matches!(self.shape_of_type(&p.ty), TypeShape::Pointer(_));
                                (is_ptr, self.fn_mutates_const_param(decl, &p.name, 0))
                            }
                            None => (false, true),
                        }
                    } else if let Some(b) = self.lookup_binding(st, name) {
                        if let Some(hy_syntax::TypeExpr::Fn { params, .. }) = &b.ty {
                            match params.get(i) {
                                Some(p) => {
                                    let is_ptr = matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. })
                                        || matches!(self.shape_of_type(&p.ty), TypeShape::Pointer(_));
                                    (is_ptr, true)
                                }
                                None => (false, true),
                            }
                        } else if matches!(b.shape, TypeShape::Closure) {
                            (false, true)
                        } else {
                            (false, false)
                        }
                    } else {
                        (false, false)
                    }
                }
                Expr::Field {
                    base,
                    name: method_name,
                    ..
                } => {
                    let is_pkg = match crate::model::unwrap_parens(base) {
                        Expr::Ident { name: pkg, .. } => {
                            self.lookup_binding(st, pkg).is_none() && self.known_package(pkg)
                        }
                        _ => false,
                    };
                    if is_pkg {
                        let mutates = match crate::model::unwrap_parens(base) {
                            Expr::Ident { name: pkg, .. } => {
                                foreign::package_mutated_arg(pkg, method_name) == Some(i)
                            }
                            _ => false,
                        };
                        (false, mutates)
                    } else if matches!(self.shape_of_expr(st, callee), TypeShape::Closure) {
                        (false, true)
                    } else {
                        let recv_base = match self.shape_of_expr(st, base) {
                            TypeShape::Struct(Some(s)) => Some(s),
                            TypeShape::Pointer(inner) => match *inner {
                                TypeShape::Struct(Some(s)) => Some(s),
                                _ => None,
                            },
                            _ => None,
                        };
                        if let Some(decl) = recv_base.and_then(|rb| {
                            self.method_decls.get(&(rb, method_name.clone())).copied()
                        }) {
                            match decl.params.get(i) {
                                Some(p) => {
                                    let is_ptr = matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. })
                                        || matches!(self.shape_of_type(&p.ty), TypeShape::Pointer(_));
                                    (is_ptr, self.fn_mutates_const_param(decl, &p.name, 0))
                                }
                                None => (false, true),
                            }
                        } else {
                            (false, false)
                        }
                    }
                }
                Expr::FnLit { params, body, .. } => match params.get(i) {
                    Some(p) => {
                        let is_ptr = matches!(p.ty, hy_syntax::TypeExpr::Pointer { .. })
                            || matches!(self.shape_of_type(&p.ty), TypeShape::Pointer(_));
                        let mut aliases = HashSet::new();
                        aliases.insert(p.name.clone());
                        (is_ptr, self.block_mutates_const_param(body, &mut aliases, 0))
                    }
                    None => (false, true),
                },
                _ => (false, false),
            };

            let const_name = self.const_arg_root(st, &a.value, declared).or_else(|| {
                if param_is_ptr && is_addressable && !already_ptr {
                    let root = inner.root_ident()?;
                    if declared.contains(root) {
                        return None;
                    }
                    let place = Place {
                        kind: PlaceKind::Ident(root.to_string()),
                        span: a.value.span(),
                    };
                    self.const_write_root(st, &place)
                } else {
                    None
                }
            });

            let Some(const_name) = const_name else {
                continue;
            };

            if mutates {
                hits.push((const_name, a.value.span()));
            }
        }
        for (const_name, span) in hits {
            self.emit_const_write_error(&const_name, span);
        }
    }

    fn fn_mutates_const_param(&self, decl: &FnDecl, param: &str, depth: usize) -> bool {
        if depth > 4 {
            return true;
        }
        let Some(body) = &decl.body else {
            return false;
        };
        let mut aliases = HashSet::new();
        aliases.insert(param.to_string());
        self.block_mutates_const_param(body, &mut aliases, depth)
    }

    fn block_mutates_const_param(
        &self,
        block: &Block,
        aliases: &mut HashSet<String>,
        depth: usize,
    ) -> bool {
        for s in &block.stmts {
            if self.stmt_mutates_const_param(s, aliases, depth) {
                return true;
            }
        }
        false
    }

    fn expr_reaches_const_aliases(expr: &Expr, aliases: &HashSet<String>) -> bool {
        let inner = crate::model::unwrap_parens(expr);
        if let Some(addr_op) = as_addr_of(inner) {
            if let Some(r) = addr_op.root_ident() {
                if aliases.contains(r) {
                    return true;
                }
            }
        }
        if let Some(r) = inner.root_ident() {
            if aliases.contains(r) {
                return true;
            }
        }
        let mut roots = Vec::new();
        collect_ref_roots(inner, &mut roots);
        collect_move_ident_sources(inner, &mut roots);
        roots.iter().any(|r| aliases.contains(r))
    }

    fn stmt_mutates_const_param(
        &self,
        stmt: &Stmt,
        aliases: &mut HashSet<String>,
        depth: usize,
    ) -> bool {
        match stmt {
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    if matches!(t.kind, PlaceKind::Index { .. } | PlaceKind::Field { .. })
                        && aliases.contains(t.root())
                    {
                        return true;
                    }
                }
                for v in values {
                    if self.expr_fnlit_mutates_const_param(v, aliases, depth) {
                        return true;
                    }
                }
                if targets.len() == values.len() {
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            if Self::expr_reaches_const_aliases(v, aliases) {
                                aliases.insert(name.clone());
                            } else {
                                aliases.remove(name);
                            }
                        } else {
                            let root = t.root();
                            if !root.is_empty() && Self::expr_reaches_const_aliases(v, aliases) {
                                aliases.insert(root.to_string());
                            }
                        }
                    }
                } else if values.len() == 1 && targets.len() > 1 {
                    let reaches = Self::expr_reaches_const_aliases(&values[0], aliases);
                    for t in targets {
                        if let PlaceKind::Ident(name) = &t.kind {
                            if reaches {
                                aliases.insert(name.clone());
                            } else {
                                aliases.remove(name);
                            }
                        } else {
                            let root = t.root();
                            if !root.is_empty() && reaches {
                                aliases.insert(root.to_string());
                            }
                        }
                    }
                }
            }
            Stmt::Local(l) => {
                for v in &l.values {
                    if self.expr_fnlit_mutates_const_param(v, aliases, depth) {
                        return true;
                    }
                }
                if l.values.is_empty() {
                    for name in &l.names {
                        aliases.remove(name);
                    }
                } else if l.names.len() == l.values.len() {
                    for (name, v) in l.names.iter().zip(l.values.iter()) {
                        if Self::expr_reaches_const_aliases(v, aliases) {
                            aliases.insert(name.clone());
                        } else {
                            aliases.remove(name);
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let reaches = Self::expr_reaches_const_aliases(&l.values[0], aliases);
                    for name in &l.names {
                        if reaches {
                            aliases.insert(name.clone());
                        } else {
                            aliases.remove(name);
                        }
                    }
                }
            }
            Stmt::Send { value, .. } => {
                if Self::expr_reaches_const_aliases(value, aliases) {
                    return true;
                }
            }
            Stmt::Task { callee, args, span } => {
                if args
                    .iter()
                    .any(|a| Self::expr_reaches_const_aliases(&a.value, aliases))
                {
                    return true;
                }
                let cr = CallRef {
                    callee,
                    args,
                    span: *span,
                };
                if self.call_mutates_const_param(&cr, aliases, depth) {
                    return true;
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    if self.expr_fnlit_mutates_const_param(v, aliases, depth) {
                        return true;
                    }
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                if self.expr_fnlit_mutates_const_param(expr, aliases, depth) {
                    return true;
                }
            }
            _ => {}
        }
        let mut calls = Vec::new();
        walk::own_calls(stmt, &mut calls);
        for cr in &calls {
            if self.call_mutates_const_param(cr, aliases, depth) {
                return true;
            }
            self.record_call_const_alias_stores(cr, aliases);
        }
        match stmt {
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                let mut branch_aliases = aliases.clone();
                if let Some(i) = init {
                    if self.stmt_mutates_const_param(i, &mut branch_aliases, depth) {
                        return true;
                    }
                }
                let mut then_aliases = branch_aliases.clone();
                if self.block_mutates_const_param(then, &mut then_aliases, depth) {
                    return true;
                }
                aliases.extend(then_aliases);
                if let Some(o) = otherwise {
                    let mut else_aliases = branch_aliases;
                    if self.stmt_mutates_const_param(o, &mut else_aliases, depth) {
                        return true;
                    }
                    aliases.extend(else_aliases);
                }
            }
            Stmt::For { kind, body, .. } => {
                let mut body_aliases = aliases.clone();
                match kind {
                    hy_syntax::ForKind::Iterate { vars, expr } => {
                        let reaches = Self::expr_reaches_const_aliases(expr, &body_aliases);
                        for v in vars {
                            if reaches {
                                body_aliases.insert(v.clone());
                            } else {
                                body_aliases.remove(v);
                            }
                        }
                    }
                    hy_syntax::ForKind::Range { var, .. } => {
                        body_aliases.remove(var);
                    }
                    _ => {}
                }
                if self.block_mutates_const_param(body, &mut body_aliases, depth) {
                    return true;
                }
                aliases.extend(body_aliases);
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                let mut body_aliases = aliases.clone();
                if self.block_mutates_const_param(body, &mut body_aliases, depth) {
                    return true;
                }
                aliases.extend(body_aliases);
            }
            Stmt::Match { subject, arms, .. } => {
                let subject_reaches = Self::expr_reaches_const_aliases(subject, aliases);
                for arm in arms {
                    let mut arm_aliases = aliases.clone();
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(bname),
                        ..
                    } = &arm.pattern
                    {
                        if subject_reaches {
                            arm_aliases.insert(bname.clone());
                        } else {
                            arm_aliases.remove(bname);
                        }
                    }
                    if self.block_mutates_const_param(&arm.body, &mut arm_aliases, depth) {
                        return true;
                    }
                    aliases.extend(arm_aliases);
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut arm_aliases = aliases.clone();
                    match &arm.kind {
                        hy_syntax::SelectArmKind::Send { value, .. } => {
                            if Self::expr_reaches_const_aliases(value, aliases) {
                                return true;
                            }
                        }
                        hy_syntax::SelectArmKind::Recv { names, .. } => {
                            for n in names {
                                arm_aliases.remove(n);
                            }
                        }
                        _ => {}
                    }
                    if self.block_mutates_const_param(&arm.body, &mut arm_aliases, depth) {
                        return true;
                    }
                    aliases.extend(arm_aliases);
                }
            }
            _ => {}
        }
        false
    }

    fn record_call_const_alias_stores(&self, cr: &CallRef, aliases: &mut HashSet<String>) {
        let callee = crate::model::unwrap_parens(cr.callee);
        match callee {
            Expr::Ident { name, .. } => {
                if name == "copy" {
                    if cr.args.len() >= 2
                        && Self::expr_reaches_const_aliases(&cr.args[1].value, aliases)
                    {
                        if let Some(dst_root) =
                            crate::model::unwrap_parens(&cr.args[0].value).root_ident()
                        {
                            aliases.insert(dst_root.to_string());
                        }
                    }
                    return;
                }
                if let Some(decl) = self.fns.get(name) {
                    let Some(body) = &decl.body else {
                        return;
                    };
                    let slots: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> = decl
                        .params
                        .iter()
                        .map(|p| (p.name.as_str(), self.shape_of_type(&p.ty), Some(&p.ty)))
                        .collect();
                    for (dst_idx, src_idx) in self.collect_callee_param_stores(&slots, body, 0) {
                        if dst_idx < cr.args.len()
                            && src_idx < cr.args.len()
                            && Self::expr_reaches_const_aliases(&cr.args[src_idx].value, aliases)
                        {
                            if let Some(dst_root) =
                                crate::model::unwrap_parens(&cr.args[dst_idx].value).root_ident()
                            {
                                aliases.insert(dst_root.to_string());
                            }
                        }
                    }
                }
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                for ((_, m), decl) in &self.method_decls {
                    if m != method_name {
                        continue;
                    }
                    let Some(body) = &decl.body else {
                        continue;
                    };
                    let mut slots: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> =
                        Vec::new();
                    let has_recv = if let Some(r) = &decl.receiver {
                        slots.push((r.name.as_str(), self.shape_of_type(&r.ty), Some(&r.ty)));
                        true
                    } else {
                        false
                    };
                    slots.extend(
                        decl.params
                            .iter()
                            .map(|p| (p.name.as_str(), self.shape_of_type(&p.ty), Some(&p.ty))),
                    );
                    let expr_at = |idx: usize| -> Option<&Expr> {
                        if has_recv {
                            if idx == 0 {
                                Some(base)
                            } else {
                                cr.args.get(idx - 1).map(|a| &a.value)
                            }
                        } else {
                            cr.args.get(idx).map(|a| &a.value)
                        }
                    };
                    for (dst_idx, src_idx) in self.collect_callee_param_stores(&slots, body, 0) {
                        if let (Some(dst_e), Some(src_e)) = (expr_at(dst_idx), expr_at(src_idx)) {
                            if Self::expr_reaches_const_aliases(src_e, aliases) {
                                if let Some(dst_root) =
                                    crate::model::unwrap_parens(dst_e).root_ident()
                                {
                                    aliases.insert(dst_root.to_string());
                                }
                            }
                        }
                    }
                }
            }
            Expr::FnLit {
                params,
                body,
                ..
            } => {
                let mut slots: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> = params
                    .iter()
                    .map(|p| (p.name.as_str(), self.shape_of_type(&p.ty), Some(&p.ty)))
                    .collect();
                let mut outer_capture_indices = Vec::new();
                for a in aliases.iter() {
                    if !params.iter().any(|p| &p.name == a) {
                        let idx = slots.len();
                        slots.push((a.as_str(), TypeShape::Unknown, None));
                        outer_capture_indices.push(idx);
                    }
                }
                for (dst_idx, src_idx) in self.collect_callee_param_stores(&slots, body, 0) {
                    if dst_idx >= cr.args.len() {
                        continue;
                    }
                    let src_reaches = if src_idx < cr.args.len() {
                        Self::expr_reaches_const_aliases(&cr.args[src_idx].value, aliases)
                    } else {
                        outer_capture_indices.contains(&src_idx)
                    };
                    if src_reaches {
                        if let Some(dst_root) =
                            crate::model::unwrap_parens(&cr.args[dst_idx].value).root_ident()
                        {
                            aliases.insert(dst_root.to_string());
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn expr_fnlit_mutates_const_param(
        &self,
        e: &Expr,
        aliases: &HashSet<String>,
        depth: usize,
    ) -> bool {
        if walk::addr_of_index_base_path(e).is_some_and(|p| aliases.contains(path_root(&p))) { return true; }
        match e {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut inner_aliases = aliases.clone();
                for p in params {
                    inner_aliases.remove(&p.name);
                }
                for r in results {
                    if let Some(name) = &r.name {
                        inner_aliases.remove(name);
                    }
                }
                if inner_aliases.is_empty() {
                    false
                } else {
                    if !params.is_empty() {
                        let mut slots: Vec<(&str, TypeShape, Option<&hy_syntax::TypeExpr>)> =
                            params
                                .iter()
                                .map(|p| (p.name.as_str(), self.shape_of_type(&p.ty), Some(&p.ty)))
                                .collect();
                        let mut cap_indices = Vec::new();
                        for a in &inner_aliases {
                            let idx = slots.len();
                            slots.push((a.as_str(), TypeShape::Unknown, None));
                            cap_indices.push(idx);
                        }
                        if self
                            .collect_callee_param_stores(&slots, body, 0)
                            .iter()
                            .any(|(dst_idx, src_idx)| {
                                *dst_idx < params.len() && cap_indices.contains(src_idx)
                            })
                        {
                            return true;
                        }
                    }
                    self.block_mutates_const_param(body, &mut inner_aliases, depth)
                }
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => {
                (matches!(as_addr_of(e).map(unwrap_parens), Some(Expr::Index { .. } | Expr::Field { .. }))
                    && Self::expr_reaches_const_aliases(e, aliases))
                    || self.expr_fnlit_mutates_const_param(inner, aliases, depth)
            }
            Expr::Call { callee, args, .. } => {
                self.expr_fnlit_mutates_const_param(callee, aliases, depth)
                    || args
                        .iter()
                        .any(|a| self.expr_fnlit_mutates_const_param(&a.value, aliases, depth))
            }
            Expr::Index { base, index, .. }
            | Expr::Binary {
                lhs: base,
                rhs: index,
                ..
            } => {
                self.expr_fnlit_mutates_const_param(base, aliases, depth)
                    || self.expr_fnlit_mutates_const_param(index, aliases, depth)
            }
            Expr::StructLit { fields, .. } => fields
                .iter()
                .any(|f| self.expr_fnlit_mutates_const_param(&f.value, aliases, depth)),
            Expr::SliceLit { items, .. } => items
                .iter()
                .any(|it| self.expr_fnlit_mutates_const_param(it, aliases, depth)),
            Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| {
                self.expr_fnlit_mutates_const_param(k, aliases, depth)
                    || self.expr_fnlit_mutates_const_param(v, aliases, depth)
            }),
            _ => false,
        }
    }

    fn call_mutates_const_param(
        &self,
        cr: &CallRef,
        aliases: &HashSet<String>,
        depth: usize,
    ) -> bool {
        let arg_reaches_alias =
            |a: &hy_syntax::CallArg| Self::expr_reaches_const_aliases(&a.value, aliases);
        match crate::model::unwrap_parens(cr.callee) {
            Expr::Ident { name, .. } => {
                if name == "delete" || name == "copy" || name == "clear" {
                    return cr.args.first().map(&arg_reaches_alias).unwrap_or(false);
                }
                if A3_PURE_FNS.contains(&name.as_str()) {
                    return false;
                }
                if let Some(decl) = self.fns.get(name) {
                    for (i, a) in cr.args.iter().enumerate() {
                        if !arg_reaches_alias(a) {
                            continue;
                        }
                        match decl.params.get(i) {
                            Some(p) => {
                                if self.fn_mutates_const_param(decl, &p.name, depth + 1) {
                                    return true;
                                }
                            }
                            None => return true,
                        }
                    }
                    return false;
                }
                cr.args.iter().any(&arg_reaches_alias)
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                if let Expr::Ident { name: pkg, .. } = &**base {
                    if A3_PURE_PKGS.contains(&pkg.as_str()) {
                        return false;
                    }
                }
                let matching: Vec<&FnDecl> = self
                    .method_decls
                    .iter()
                    .filter(|((_, m), _)| m == method_name)
                    .map(|(_, d)| *d)
                    .collect();
                if Self::expr_reaches_const_aliases(base, aliases)
                    && ((method_name == "pop"
                        && cr.args.is_empty()
                        && !matches!(crate::model::unwrap_parens(base), Expr::Ident { .. }))
                        || matching.is_empty()
                        || matching.iter().any(|d| {
                            d.receiver
                                .as_ref()
                                .is_some_and(|r| self.fn_mutates_const_param(d, &r.name, depth + 1))
                        }))
                {
                    return true;
                }
                if !matching.is_empty() {
                    for (i, a) in cr.args.iter().enumerate() {
                        if !arg_reaches_alias(a) {
                            continue;
                        }
                        if matching.iter().any(|d| match d.params.get(i) {
                            Some(p) => self.fn_mutates_const_param(d, &p.name, depth + 1),
                            None => true,
                        }) {
                            return true;
                        }
                    }
                    return false;
                }
                cr.args.iter().any(&arg_reaches_alias)
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                if depth > 4 {
                    return true;
                }
                let mut inner_aliases = aliases.clone();
                for p in params {
                    inner_aliases.remove(&p.name);
                }
                for r in results {
                    if let Some(name) = &r.name {
                        inner_aliases.remove(name);
                    }
                }
                for (i, a) in cr.args.iter().enumerate() {
                    if !arg_reaches_alias(a) {
                        continue;
                    }
                    match params.get(i) {
                        Some(p) => {
                            inner_aliases.insert(p.name.clone());
                        }
                        None => return true,
                    }
                }
                if inner_aliases.is_empty() {
                    false
                } else {
                    self.block_mutates_const_param(body, &mut inner_aliases, depth + 1)
                }
            }
            _ => cr.args.iter().any(&arg_reaches_alias),
        }
    }

    /// The shape of an assignment place, when it resolves (drives the N1
    /// nil checks on assignment targets).
    pub(crate) fn place_shape(&self, st: &FnState, p: &Place) -> Option<TypeShape> {
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

    /// The declared type of an assignment place, when it resolves (drives
    /// HYB-E6002 constant overflow checks on assignment targets).
    // ponytail: resolves direct ident, index, and struct field targets via package struct definitions.
    pub(crate) fn place_target_ty(&self, st: &FnState, p: &Place) -> Option<hy_syntax::TypeExpr> {
        enum Step<'a> {
            Index,
            Field(&'a str),
        }
        let mut steps = Vec::new();
        let mut walker = p;
        loop {
            match &walker.kind {
                PlaceKind::Index { base, .. } => {
                    steps.push(Step::Index);
                    walker = base;
                }
                PlaceKind::Field { base, name } => {
                    steps.push(Step::Field(name.as_str()));
                    walker = base;
                }
                PlaceKind::AddrOf { base } => {
                    walker = base;
                }
                PlaceKind::Ident(name) => {
                    let b = self.lookup_binding(st, name)?;
                    let mut cur = b.ty.as_ref()?;
                    steps.reverse();
                    for step in steps {
                        while let hy_syntax::TypeExpr::Pointer { to, .. } = cur {
                            cur = to.as_ref();
                        }
                        match step {
                            Step::Index => match cur {
                                hy_syntax::TypeExpr::Array { elem, .. }
                                | hy_syntax::TypeExpr::Slice { elem, .. } => {
                                    cur = elem.as_ref();
                                }
                                hy_syntax::TypeExpr::Map { value, .. } => {
                                    cur = value.as_ref();
                                }
                                _ => return None,
                            },
                            Step::Field(field_name) => match cur {
                                hy_syntax::TypeExpr::Named {
                                    name: struct_name, ..
                                } => {
                                    let fields = self.structs.get(struct_name)?;
                                    let (_, field_ty) =
                                        fields.iter().find(|(f, _)| f == field_name)?;
                                    cur = field_ty;
                                }
                                _ => return None,
                            },
                        }
                    }
                    return Some(cur.clone());
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Returns (LOAN-ESCAPE + move marking)
    // -----------------------------------------------------------------------

    pub(crate) fn process_return(&mut self, st: &mut FnState, values: &[Expr], span: Span) {
        if values.is_empty() {
            // Bare `return` in a function with named results implicitly
            // returns every named result binding (`st.named_results`).
            for name in st.named_results.clone() {
                let is_loan_escape = st.borrow_bindings.contains(&name)
                    || st
                        .loans
                        .iter()
                        .any(|l| matches!(&l.target, LoanTarget::MapSlot { binding, .. } if binding == &name))
                    || st
                        .env
                        .get(&name)
                        .is_some_and(|b| matches!(b.shape, TypeShape::BorrowHandle(_)));
                if is_loan_escape {
                    let msg = "a borrow cannot be returned from a function".to_string();
                    let notes = vec![
                        format!(
                            "`{}` holds a loan whose window ends when this function exits — Tier 0 keeps every loan function-local (O2): that is why there are no lifetime annotations to write",
                            name
                        ),
                        "make the mutation a method on the owner so the loan stays in one frame, return the index and let the caller do the access, or return a copy (Go-heap reads are copies, O9)".to_string(),
                        "a lifetime parameter is NOT offered on purpose: the language has none".to_string(),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E3104", self.path.clone(), span, msg)
                            .with_notes(notes),
                    );
                }
            }
            return;
        }
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
            let is_bound_borrow = match unwrap_parens(v) {
                Expr::Ident { name, .. } => {
                    st.borrow_bindings.contains(name)
                        || st
                            .loans
                            .iter()
                            .any(|l| matches!(&l.target, LoanTarget::MapSlot { binding, .. } if binding == name))
                }
                _ => false,
            };
            // O2/LOAN-ESCAPE: a borrow returned from a function
            if as_addr_of(v).is_some()
                || contains_addr_of_in_literal(v)
                || is_bound_borrow
                || self.expr_carries_borrow(st, v)
            {
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

/// Collect root identifiers of `move` operands inside `expr` (including
/// top-level `move x` and `move` expressions nested inside composite literals
/// or call arguments).
fn collect_move_ident_sources(expr: &Expr, out: &mut Vec<String>) {
    match unwrap_parens(expr) {
        Expr::Move { operand, .. } => {
            if let Some(root) = unwrap_parens(operand).root_ident() {
                out.push(root.to_string());
            }
            collect_move_ident_sources(operand, out);
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_move_ident_sources(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for it in items {
                collect_move_ident_sources(it, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_move_ident_sources(k, out);
                collect_move_ident_sources(v, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            collect_move_ident_sources(callee, out);
            for a in args {
                collect_move_ident_sources(&a.value, out);
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_move_ident_sources(base, out);
            if let Some(lo) = lo {
                collect_move_ident_sources(lo, out);
            }
            if let Some(hi) = hi {
                collect_move_ident_sources(hi, out);
            }
        }
        Expr::Field { base, .. }
        | Expr::Unary { operand: base, .. }
        | Expr::AddrOf { operand: base, .. }
        | Expr::Try { operand: base, .. } => {
            collect_move_ident_sources(base, out);
        }
        Expr::Index { base, index, .. } | Expr::Binary { lhs: base, rhs: index, .. } => {
            collect_move_ident_sources(base, out);
            collect_move_ident_sources(index, out);
        }
        _ => {}
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
// #109: closure-capture ident collection (may-share edges at definition)
// ---------------------------------------------------------------------------

/// All identifier uses inside a closure body that may resolve to an
/// enclosing binding. `bound` holds names shadowed by the closure's own
/// parameters (and nested closures'), so those idents are not captures.
/// Locals defined inside the body are deliberately NOT excluded: a name
/// that also exists outside resolves to the outer binding on other paths,
/// and an extra may-share edge is conservative in the safe direction.
pub(crate) fn collect_capture_idents_block(
    block: &hy_syntax::Block,
    bound: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    for s in &block.stmts {
        collect_capture_idents_stmt(s, bound, out);
    }
}

fn collect_capture_idents_stmt(
    stmt: &hy_syntax::Stmt,
    bound: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    match stmt {
        hy_syntax::Stmt::Local(l) => {
            for v in &l.values {
                collect_capture_idents_expr(v, bound, out);
            }
        }
        hy_syntax::Stmt::Assign { targets, values, .. } => {
            for t in targets {
                collect_capture_idents_place(t, bound, out);
            }
            for v in values {
                collect_capture_idents_expr(v, bound, out);
            }
        }
        hy_syntax::Stmt::Expr { expr, .. } => collect_capture_idents_expr(expr, bound, out),
        hy_syntax::Stmt::Send { chan, value, .. } => {
            collect_capture_idents_expr(chan, bound, out);
            collect_capture_idents_expr(value, bound, out);
        }
        hy_syntax::Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                collect_capture_idents_stmt(i, bound, out);
            }
            collect_capture_idents_expr(cond, bound, out);
            collect_capture_idents_block(then, bound, out);
            if let Some(o) = otherwise {
                collect_capture_idents_stmt(o, bound, out);
            }
        }
        hy_syntax::Stmt::For { kind, body, .. } => {
            match kind {
                hy_syntax::ForKind::While { cond } => {
                    collect_capture_idents_expr(cond, bound, out)
                }
                hy_syntax::ForKind::Range { lo, hi, .. } => {
                    collect_capture_idents_expr(lo, bound, out);
                    collect_capture_idents_expr(hi, bound, out);
                }
                hy_syntax::ForKind::Iterate { expr, .. } => {
                    collect_capture_idents_expr(expr, bound, out)
                }
                hy_syntax::ForKind::Infinite => {}
            }
            collect_capture_idents_block(body, bound, out);
        }
        hy_syntax::Stmt::Return { values, .. } => {
            for v in values {
                collect_capture_idents_expr(v, bound, out);
            }
        }
        hy_syntax::Stmt::Task { callee, args, .. } => {
            collect_capture_idents_expr(callee, bound, out);
            for a in args {
                collect_capture_idents_expr(&a.value, bound, out);
            }
        }
        hy_syntax::Stmt::Managed { body, .. } => collect_capture_idents_block(body, bound, out),
        hy_syntax::Stmt::Match { subject, arms, .. } => {
            collect_capture_idents_expr(subject, bound, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_capture_idents_expr(g, bound, out);
                }
                collect_capture_idents_block(&arm.body, bound, out);
            }
        }
        hy_syntax::Stmt::Defer { call, .. } => collect_capture_idents_expr(call, bound, out),
        hy_syntax::Stmt::Break { .. } | hy_syntax::Stmt::Continue { .. } => {}
        hy_syntax::Stmt::Block { block, .. } => collect_capture_idents_block(block, bound, out),
        hy_syntax::Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    hy_syntax::SelectArmKind::Send { chan, value, .. } => {
                        collect_capture_idents_expr(chan, bound, out);
                        collect_capture_idents_expr(value, bound, out);
                    }
                    hy_syntax::SelectArmKind::Recv { chan, .. } => {
                        collect_capture_idents_expr(chan, bound, out)
                    }
                    hy_syntax::SelectArmKind::Default => {}
                }
                collect_capture_idents_block(&arm.body, bound, out);
            }
        }
    }
}

fn collect_capture_idents_expr(
    expr: &Expr,
    bound: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    match expr {
        Expr::Ident { name, .. } => {
            if !bound.contains(name) {
                out.push(name.clone());
            }
        }
        Expr::FnLit {
            params,
            results,
            body,
            ..
        } => {
            let mut nested = bound.clone();
            for p in params {
                nested.insert(p.name.clone());
            }
            for r in results {
                if let Some(name) = &r.name {
                    nested.insert(name.clone());
                }
            }
            collect_capture_idents_block(body, &mut nested, out);
        }
        Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let hy_syntax::InterpPart::Expr(e) = p {
                    collect_capture_idents_expr(e, bound, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_capture_idents_expr(lhs, bound, out);
            collect_capture_idents_expr(rhs, bound, out);
        }
        Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } => {
            collect_capture_idents_expr(operand, bound, out)
        }
        Expr::Move { operand, .. } | Expr::Try { operand, .. } => {
            collect_capture_idents_expr(operand, bound, out)
        }
        Expr::Receive { chan, .. } => collect_capture_idents_expr(chan, bound, out),
        Expr::Index { base, index, .. } => {
            collect_capture_idents_expr(base, bound, out);
            collect_capture_idents_expr(index, bound, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_capture_idents_expr(base, bound, out);
            if let Some(lo) = lo {
                collect_capture_idents_expr(lo, bound, out);
            }
            if let Some(hi) = hi {
                collect_capture_idents_expr(hi, bound, out);
            }
        }
        Expr::Field { base, .. } => collect_capture_idents_expr(base, bound, out),
        Expr::Call { callee, args, .. } => {
            collect_capture_idents_expr(callee, bound, out);
            for a in args {
                collect_capture_idents_expr(&a.value, bound, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_capture_idents_expr(&f.value, bound, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                collect_capture_idents_expr(i, bound, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_capture_idents_expr(k, bound, out);
                collect_capture_idents_expr(v, bound, out);
            }
        }
        Expr::Paren { inner, .. } => collect_capture_idents_expr(inner, bound, out),
    }
}

fn collect_capture_idents_place(
    place: &Place,
    bound: &mut std::collections::HashSet<String>,
    out: &mut Vec<String>,
) {
    match &place.kind {
        PlaceKind::Ident(name) => {
            if !bound.contains(name) {
                out.push(name.clone());
            }
        }
        PlaceKind::Index { base, index } => {
            collect_capture_idents_place(base, bound, out);
            collect_capture_idents_expr(index, bound, out);
        }
        PlaceKind::Field { base, .. } => collect_capture_idents_place(base, bound, out),
        PlaceKind::AddrOf { base } => collect_capture_idents_place(base, bound, out),
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
                match field_borrow_kind(&shape) {
                    Some("borrow-handle") => {
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
                    Some("pointer") => {
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
        let non_owned_origin = Some(format!("file-level `const {}`", c.name));
        Binding {
            shape,
            ty,
            immutable: true, // class 4: const values are immutable-by-type
            is_let: false,
            managed_taint: false,
            moved: None,
            non_owned_origin,
            slot_loan: false,
        }
    }
}

fn field_borrow_kind(shape: &TypeShape) -> Option<&'static str> {
    match shape {
        TypeShape::BorrowHandle(_) => Some("borrow-handle"),
        TypeShape::Pointer(inner) if !matches!(**inner, TypeShape::Atomic) => Some("pointer"),
        TypeShape::Slice(elem) | TypeShape::Array(elem) => field_borrow_kind(elem),
        TypeShape::Map(k, v) => field_borrow_kind(k).or_else(|| field_borrow_kind(v)),
        _ => None,
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
        // Fixture seam-fix for #134: `n` is only stored to (a bare `=`
        // target is not a Go read), so `_ = n` keeps the snippet a valid
        // program and the E3001 spawn check first.
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
               _ = n\n\
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

    #[test]
    fn move_file_level_const_composite_rejects_with_e3004() {
        let r = check(
            "const m = map[string]int{\"init\": 0}\n\
             fn worker(x map[string]int) { x[\"k\"] = 1 }\n\
             fn main() {\n\
               task worker(move m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for file-level const composite move, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn move_file_level_const_composite_post_move_use_also_emits_e3001() {
        let r = check(
            "const m = map[string]int{\"init\": 0}\n\
             fn worker(x map[string]int) { x[\"k\"] = 1 }\n\
             fn main() {\n\
               task worker(move m)\n\
               m[\"x\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for file-level const composite move, got {:?}",
            r.diagnostics
        );
        // AMENDMENT (pr-128 integration, logged): on the merged seam the
        // post-move write is named by E3003 (use-after-move, the fef40da-era
        // use checker) and E3006 (const deep immutability, pr-44) — the
        // contributor's E3001 expectation assumed its base's boundary pass
        // would name it. The requirement stays "the post-move violation is
        // diagnosed": either use-after-move or the const-write guard.
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E3001" || d.code == "HYB-E3003"),
            "expected a post-move violation diagnostic (E3001 or E3003), got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn move_local_const_composite_rejects_with_e3004() {
        let r = check(
            "fn worker(x map[string]int) { x[\"k\"] = 1 }\n\
             fn main() {\n\
               const m = map[string]int{\"init\": 0}\n\
               task worker(move m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for local const composite move, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn move_local_copy_of_file_level_const_composite_rejects_with_e3004() {
        let r = check(
            "struct holder { m map[string]int }\n\
             const h = holder{m: map[string]int{\"init\": 0}}\n\
             fn worker(x holder) { x.m[\"k\"] = 1 }\n\
             fn main() {\n\
               h2 := h\n\
               task worker(move h2)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for local struct copy of file-level const composite, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn move_sendable_scalar_const_still_accepts() {
        let r = check(
            "const k = 42\n\
             fn worker(n int) {}\n\
             fn main() {\n\
               const local_k = 7\n\
               task worker(move k)\n\
               task worker(move local_k)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn param_may_share_peer_survived_inner_shadow_rejects_with_e3004() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder, done chan int) {\n\
               h.m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(h holder, done chan int) {\n\
               h2 := h\n\
               if true {\n\
                 h := holder{m: map[string]int{\"fresh\": 0}}\n\
                 _ = h\n\
                 task worker(move h2, done)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 when moving parameter may-share peer after inner shadow, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn param_may_share_peer_survived_parallel_redefine_rejects_with_e3004() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder, done chan int) {\n\
               h.m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(h holder, done chan int) {\n\
               h2 := h\n\
               h, tag := holder{m: map[string]int{\"fresh\": 0}}, 1\n\
               _ = h\n\
               _ = tag\n\
               task worker(move h2, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 when moving parameter may-share peer after multi-define, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn multi_return_define_from_param_rejects_may_share_move_with_e3004() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn alias_pair(h holder) (holder, bool) {\n\
               return h, true\n\
             }\n\
             fn worker(h holder, done chan int) {\n\
               h.m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(h holder, done chan int) {\n\
               h2, ok := alias_pair(h)\n\
               _ = ok\n\
               task worker(move h2, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for multi-return may-share peer of parameter, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn multi_return_assign_from_param_rejects_may_share_move_with_e3004() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn alias_pair(h holder) (holder, bool) {\n\
               return h, true\n\
             }\n\
             fn worker(h holder, done chan int) {\n\
               h.m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(h holder, done chan int) {\n\
               h2 := holder{m: map[string]int{\"fresh\": 0}}\n\
               ok := false\n\
               h2, ok = alias_pair(h)\n\
               _ = ok\n\
               task worker(move h2, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for multi-return rebind may-share peer of parameter, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn comma_ok_map_lookup_on_param_rejects_move_with_e3004() {
        let r = check(
            "fn worker(m map[string]int, done chan int) {\n\
               m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(buckets map[string]map[string]int, done chan int) {\n\
               m, ok := buckets[\"k\"]\n\
               _ = ok\n\
               task worker(move m, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "expected HYB-E3004 for comma-ok map lookup on non-owned parameter, got {:?}",
            r.diagnostics
        );
    }

    #[test]
    fn multi_return_value_only_args_accept_move_control() {
        let r = check(
            "struct holder { m map[string]int }\n\
             fn pair(x int) (int, bool) {\n\
               return x, true\n\
             }\n\
             fn make_holder(seed int) (holder, bool) {\n\
               return holder{m: map[string]int{\"k\": seed}}, true\n\
             }\n\
             fn worker(h holder, done chan int) {\n\
               h.m[\"worker\"] = 1\n\
               done <- 1\n\
             }\n\
             fn outer(seed int, done chan int) {\n\
               n, ok1 := pair(42)\n\
               _ = n\n\
               _ = ok1\n\
               h2, ok2 := make_holder(seed)\n\
               _ = ok2\n\
               task worker(move h2, done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    fn check_codes(src: &str) -> Vec<String> {
        let r = check(src);
        r.diagnostics.into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn block_scoped_closure_capture_rejects_shadowed_map_spawn() {
        let src = "\
fn worker(m map[string]int, done chan int) {
    m[\"k\"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    m := 1
    _ = m
    if true {
        m := map[string]int{\"k\": 0}
        f := fn() {
            task worker(m, done)
        }
        f()
        m[\"k\"] = 2
    }
    <-done
}
";
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E3001"),
            "expected HYB-E3001 for block-shadowed map capture, got {:?}",
            codes
        );
    }

    #[test]
    fn block_scoped_closure_rejects_move_of_ascribed_alias() {
        let src = "\
fn worker(m map[string]int, done chan int) {
    m[\"k\"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 2)
    if true {
        m := map[string]int{\"k\": 0}
        f := fn() {
            var alias map[string]int = m
            task worker(move alias, done)
        }
        f()
        f()
    }
    <-done
    <-done
}
";
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E3004"),
            "expected HYB-E3004 for ascribed alias move of block capture, got {:?}",
            codes
        );
    }

    #[test]
    fn block_scoped_closure_rejects_use_after_move_and_const_write() {
        let src_uam = "\
fn worker(m map[string]int, done chan int) {
    m[\"k\"] = 1
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    if true {
        m := map[string]int{\"k\": 0}
        task worker(move m, done)
        f := fn() {
            m[\"k\"] = 99
        }
        f()
    }
    <-done
}
";
        let codes_uam = check_codes(src_uam);
        assert!(
            codes_uam.iter().any(|c| c == "HYB-E3003"),
            "expected HYB-E3003 for moved block-scoped map captured in closure, got {:?}",
            codes_uam
        );

        let src_const = "\
fn main() {
    if true {
        const c = map[string]int{\"k\": 0}
        f := fn() {
            c[\"k\"] = 1
        }
        f()
    }
}
";
        let codes_const = check_codes(src_const);
        assert!(
            codes_const.iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006 for block-scoped const write in closure, got {:?}",
            codes_const
        );
    }

    #[test]
    fn const_map_and_struct_slot_loan_rejected_as_const_write() {
        let src_map = "\
struct svc {
    count int
}

const shared = map[string]svc{\"a\": svc{count: 1}}

fn worker(done chan int) {
    s := &shared[\"a\"]
    _ = s.count
    done <- 1
}
";
        let codes_map = check_codes(src_map);
        assert!(
            codes_map.iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006 for O8 slot loan on const map, got {:?}",
            codes_map
        );

        let src_struct = "\
struct inner {
    v int
}

struct cfg {
    s inner
}

const global_cfg = cfg{s: inner{v: 1}}

fn main() {
    p := &global_cfg.s
    _ = p.v
}
";
        let codes_struct = check_codes(src_struct);
        assert!(
            codes_struct.iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006 for O8 field loan on const struct, got {:?}",
            codes_struct
        );
    }

    #[test]
    fn const_array_and_struct_pointer_borrow_enforces_const_and_origin() {
        let src_arr = "\
const arr [2]int = [10, 20]

fn main() {
    p := &arr
    p[0] = 99
}
";
        let codes_arr = check_codes(src_arr);
        assert!(
            codes_arr.iter().any(|c| c == "HYB-E3006"),
            "expected HYB-E3006 for write through pointer borrow of const array, got {:?}",
            codes_arr
        );

        let src_spawn = "\
struct cfg {
    count int
}

const global_cfg = cfg{count: 1}

fn worker(p *cfg) {
    p.count = 99
}

fn main() {
    p := &global_cfg
    task worker(move p)
}
";
        let codes_spawn = check_codes(src_spawn);
        assert!(
            codes_spawn.iter().any(|c| c == "HYB-E3004"),
            "expected HYB-E3004 for moving pointer borrow of file-level const struct, got {:?}",
            codes_spawn
        );
    }

    #[test]
    fn slot_loan_alias_extends_a3_window() {
        let src = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := &m[\"a\"]
    p := s
    m[\"b\"] = svc{count: 2}
    p.count = 99
}
";
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 when map is mutated before slot-loan alias use, got {:?}",
            codes
        );
    }

    #[test]
    fn loop_carried_slot_loan_rejects_post_use_insert_in_loop() {
        let src = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := &m[\"a\"]
    for i in 0..3 {
        s.count += i
        m[\"b\"] = svc{count: i}
    }
}
";
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for loop-carried slot loan invalidated before next iteration, got {:?}",
            codes
        );
    }

    #[test]
    fn user_method_and_callee_alias_and_closure_slot_loan_enforce_a3() {
        let src_method = "\
struct svc {
    count int
}

struct env {
    m map[string]svc
}

fn (e *env) bump() {
    e.m[\"b\"] = svc{count: 2}
}

fn main() {
    e := env{m: map[string]svc{\"a\": svc{count: 1}}}
    s := &e.m[\"a\"]
    e.bump()
    s.count = 99
}
";
        assert!(
            check_codes(src_method).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for user-defined method mutating loaned map"
        );

        let src_alias_fn = "\
struct svc {
    count int
}

fn mutate_via_alias(t map[string]svc) {
    alias := t
    alias[\"b\"] = svc{count: 2}
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := &m[\"a\"]
    mutate_via_alias(m)
    s.count = 99
}
";
        assert!(
            check_codes(src_alias_fn).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for callee mutating parameter through local alias"
        );

        let src_closure_loan = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    f := fn() {
        s := &m[\"a\"]
        m[\"b\"] = svc{count: 2}
        s.count = 99
    }
    f()
}
";
        assert!(
            check_codes(src_closure_loan).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for map-slot loan invalidated inside closure body"
        );
    }

    #[test]
    fn parenthesized_addr_of_enforces_a3_and_a6() {
        let src_a3 = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := (&m[\"a\"])
    m[\"b\"] = svc{count: 2}
    s.count = 99
}
";
        assert!(
            check_codes(src_a3).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for parenthesized (&m[\"a\"])"
        );

        let src_a3_inner = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := &(m[\"a\"])
    m[\"b\"] = svc{count: 2}
    s.count = 99
}
";
        assert!(
            check_codes(src_a3_inner).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for inner-parenthesized &(m[\"a\"])"
        );

        let src_a3_closure = "\
struct svc {
    count int
}

fn main() {
    m := map[string]svc{\"a\": svc{count: 1}}
    s := (&m[\"a\"])
    f := fn() {
        s.count = 99
    }
    m[\"b\"] = svc{count: 2}
    f()
}
";
        assert!(
            check_codes(src_a3_closure).iter().any(|c| c == "HYB-E3102"),
            "expected HYB-E3102 for parenthesized (&m[\"a\"]) captured by closure across map insert"
        );

        let src_a6 = "\
struct holder {
    p *int
}

fn main() {
    var h holder
    x := 1
    h.p = &(x)
}
";
        assert!(
            check_codes(src_a6).iter().any(|c| c == "HYB-E3103"),
            "expected HYB-E3103 for parenthesized &(x) stored in struct field"
        );
    }
}

