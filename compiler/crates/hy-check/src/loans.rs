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
    as_addr_of, collect_addr_of_roots, collect_ref_roots, move_operand_ident, unwrap_parens,
    Binding, Checker, FnState, Loan, LoanTarget, MoveKind, TypeShape,
};
use crate::shape::receiver_type_name;
use crate::walk::{self, CallRef};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Block, Expr, FnDecl, ForKind, LocalDecl, LocalKind, MatchPattern, Place, PlaceKind,
    Stmt,
};
use std::collections::{HashMap, HashSet};

/// Read-only call targets that can never mutate a map argument — the A3
/// callee scan fails closed on everything else (#111).
const A3_PURE_FNS: &[&str] = &["len", "cap", "print", "println"];
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
                                let active_handle = self
                                    .lookup_binding(st, root)
                                    .map(|b| b.shape.is_handle() && b.moved.is_none())
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
                                let active_handle = self
                                    .lookup_binding(st, root)
                                    .map(|b| b.shape.is_handle() && b.moved.is_none())
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
                if name == "delete" {
                    // A3: map delete is a map mutation
                    self.check_delete_conflict(st, cr, idx);
                }
                if name == "delete" || name == "copy" {
                    // T22 (#123/#188): a delete or copy mutates its first
                    // argument — a `const` root is frozen like an index write.
                    self.check_delete_const(st, cr, &HashSet::new());
                }
                self.check_call_const_writes(st, cr, &HashSet::new());
                if self.fns.contains_key(&name) {
                    // A3 #111: the callee may insert/delete through the
                    // argument — that event runs inside THIS call site's
                    // window when the call is within it.
                    self.check_a3_call_conflict(st, cr, idx, &name);
                    // A7 borrow-by-default: a handle passed to a Hybrid
                    // function is lent for the call only
                    self.lend_handle_args(st, cr, idx, None);
                    // O5.2 (#455): a helper may retain one reference-bearing
                    // argument through another (`dst[0] = v` inside the
                    // callee). The callee's own FnState records that edge;
                    // the caller's never sees it, so a later `move` would not
                    // freeze the caller-visible alias. Materialize the edge at
                    // the call site from the callee's argument-store effect.
                    if let Some(decl) = self.fns.get(&name).copied() {
                        for (di, si) in self.collect_param_store_pairs(decl, 0) {
                            let (Some(dst_arg), Some(src_arg)) = (cr.args.get(di), cr.args.get(si))
                            else {
                                continue;
                            };
                            if let Some(dst_root) = dst_arg.value.root_ident() {
                                self.record_may_share(st, dst_root, &src_arg.value);
                            }
                        }
                    }
                } else if st.closure_mutates.contains_key(&name) {
                    // A3 #111 form 4: a local closure whose body mutates a
                    // captured binding — event attributed to this call site.
                    self.check_a3_closure_call(st, cr, idx, &name);
                }
            }
            Resolved::None => {
                self.check_call_const_writes(st, cr, &HashSet::new());
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
                if let Some(b) = self.lookup_binding(st, name) {
                    if b.shape.is_handle() && b.moved.is_none() {
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
    /// Matching is peer-aware (#111): an event on `alias` conflicts with a
    /// loan on `m` when the roots share storage.
    fn emit_a3_if_live(&mut self, st: &FnState, path: &str, span: Span, idx: usize, verb: &str) {
        let Some(loan) = st.loans.iter().find(|l| {
            matches!(&l.target, LoanTarget::MapSlot { path: p, .. }
                if a3_paths_conflict(st, p, path))
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
            .filter_map(|l| match &l.target {
                LoanTarget::MapSlot { path, .. } if l.created <= idx && idx <= l.last => {
                    Some(path.clone())
                }
                _ => None,
            })
            .collect();
        if live.is_empty() {
            return;
        }
        let mut conflict_paths: Vec<String> = Vec::new();
        for (i, a) in cr.args.iter().enumerate() {
            let Some(arg_path) = walk::expr_path(&a.value) else {
                continue;
            };
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
        let verb = format!("mutation via `{}`", cr.callee.render());
        for p in conflict_paths {
            self.emit_a3_if_live(st, &p, cr.span, idx, &verb);
        }
    }

    /// #111 form 4: a local closure invoked inside a live slot-loan window
    /// whose recorded mutation roots reach the loaned storage.
    fn check_a3_closure_call(&mut self, st: &FnState, cr: &CallRef, idx: usize, name: &str) {
        let Some(roots) = st.closure_mutates.get(name) else {
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
            if !(l.created <= idx && idx <= l.last) {
                continue;
            }
            if roots.iter().any(|r| a3_paths_conflict(st, path, r)) {
                self.emit_a3_if_live(st, path, cr.span, idx, &verb);
                return;
            }
        }
    }

    // -----------------------------------------------------------------
    // O5.2 caller-side argument-store effects (#455)
    // -----------------------------------------------------------------

    /// Pairs `(dst, src)` of parameter indices such that a declared helper
    /// stores a value rooted at parameter `src` into a place rooted at
    /// parameter `dst` — directly, through a local alias, through a nested
    /// block/loop/match, through a capturing closure body, or through a
    /// nested declared call (depth-limited, like `fn_mutates_param`).
    fn collect_param_store_pairs(&self, decl: &FnDecl, depth: usize) -> Vec<(usize, usize)> {
        if depth > 3 {
            return Vec::new();
        }
        let Some(body) = &decl.body else {
            return Vec::new();
        };
        let mut alias: HashMap<String, usize> = HashMap::new();
        for (i, p) in decl.params.iter().enumerate() {
            alias.insert(p.name.clone(), i);
        }
        let mut out = Vec::new();
        self.stmts_store_pairs(&body.stmts, &mut alias, depth, &mut out);
        out.sort_unstable();
        out.dedup();
        out
    }

    fn stmts_store_pairs(
        &self,
        stmts: &[Stmt],
        alias: &mut HashMap<String, usize>,
        depth: usize,
        out: &mut Vec<(usize, usize)>,
    ) {
        for s in stmts {
            self.stmt_store_pairs(s, alias, depth, out);
        }
    }

    fn stmt_store_pairs(
        &self,
        s: &Stmt,
        alias: &mut HashMap<String, usize>,
        depth: usize,
        out: &mut Vec<(usize, usize)>,
    ) {
        // Calls anywhere in this statement may store through their arguments.
        let mut calls = Vec::new();
        walk::own_calls(s, &mut calls);
        self.calls_store_pairs(&calls, alias, depth, out);

        match s {
            Stmt::Local(l) => {
                if l.values.len() == l.names.len() {
                    for (n, v) in l.names.iter().zip(l.values.iter()) {
                        match v.root_ident().and_then(|r| alias.get(r).copied()) {
                            Some(p) => {
                                alias.insert(n.clone(), p);
                            }
                            None => {
                                alias.remove(n);
                            }
                        }
                    }
                } else {
                    // multi-value define / no initializer: names are fresh
                    for n in &l.names {
                        alias.remove(n);
                    }
                }
                for v in &l.values {
                    self.expr_fnlit_store_pairs(v, alias, depth, out);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for (t, v) in targets.iter().zip(values.iter()) {
                    match &t.kind {
                        PlaceKind::Ident(name) => {
                            // rebinding a name keeps an alias only if the RHS
                            // still roots at the same parameter
                            match v.root_ident().and_then(|r| alias.get(r).copied()) {
                                Some(p) => {
                                    alias.insert(name.clone(), p);
                                }
                                None => {
                                    alias.remove(name);
                                }
                            }
                        }
                        _ => {
                            let dst = alias.get(t.root()).copied();
                            let src = v.root_ident().and_then(|r| alias.get(r).copied());
                            if let (Some(dst), Some(src)) = (dst, src) {
                                if dst != src {
                                    out.push((dst, src));
                                }
                            }
                        }
                    }
                }
                for v in values {
                    self.expr_fnlit_store_pairs(v, alias, depth, out);
                }
            }
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.stmt_store_pairs(i, alias, depth, out);
                }
                let mut t = alias.clone();
                self.stmts_store_pairs(&then.stmts, &mut t, depth, out);
                if let Some(o) = otherwise {
                    let mut e = alias.clone();
                    self.stmt_store_pairs(o, &mut e, depth, out);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => {
                        let mut c = alias.clone();
                        self.expr_store_pairs(cond, &mut c, depth, out);
                    }
                    ForKind::Iterate { expr, .. } => {
                        let mut c = alias.clone();
                        self.expr_store_pairs(expr, &mut c, depth, out);
                    }
                    ForKind::Range { lo, hi, .. } => {
                        let mut c = alias.clone();
                        self.expr_store_pairs(lo, &mut c, depth, out);
                        let mut c2 = alias.clone();
                        self.expr_store_pairs(hi, &mut c2, depth, out);
                    }
                    ForKind::Infinite => {}
                }
                let mut b = alias.clone();
                self.stmts_store_pairs(&body.stmts, &mut b, depth, out);
            }
            Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
                let mut b = alias.clone();
                self.stmts_store_pairs(&block.stmts, &mut b, depth, out);
            }
            Stmt::Match { subject, arms, .. } => {
                let subj = subject.root_ident().and_then(|r| alias.get(r).copied());
                for arm in arms {
                    let mut a = alias.clone();
                    if let (
                        Some(p),
                        MatchPattern::Path {
                            binding: Some(b), ..
                        },
                    ) = (subj, &arm.pattern)
                    {
                        a.insert(b.clone(), p);
                    }
                    if let Some(g) = &arm.guard {
                        let mut ga = a.clone();
                        self.expr_store_pairs(g, &mut ga, depth, out);
                    }
                    self.stmts_store_pairs(&arm.body.stmts, &mut a, depth, out);
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let mut a = alias.clone();
                    self.stmts_store_pairs(&arm.body.stmts, &mut a, depth, out);
                }
            }
            _ => {}
        }
    }

    fn expr_store_pairs(
        &self,
        e: &Expr,
        alias: &mut HashMap<String, usize>,
        depth: usize,
        out: &mut Vec<(usize, usize)>,
    ) {
        let mut calls = Vec::new();
        walk::expr_calls(e, &mut calls);
        self.calls_store_pairs(&calls, alias, depth, out);
        self.expr_fnlit_store_pairs(e, alias, depth, out);
    }

    /// A closure body inside the callee can capture a parameter and store it;
    /// its own parameters/results shadow the capture.
    fn expr_fnlit_store_pairs(
        &self,
        e: &Expr,
        alias: &mut HashMap<String, usize>,
        depth: usize,
        out: &mut Vec<(usize, usize)>,
    ) {
        match e {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                let mut b = alias.clone();
                for p in params {
                    b.remove(&p.name);
                }
                for r in results {
                    if let Some(n) = &r.name {
                        b.remove(n);
                    }
                }
                self.stmts_store_pairs(&body.stmts, &mut b, depth, out);
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => {
                self.expr_fnlit_store_pairs(inner, alias, depth, out)
            }
            Expr::Call { callee, args, .. } => {
                self.expr_fnlit_store_pairs(callee, alias, depth, out);
                for a in args {
                    self.expr_fnlit_store_pairs(&a.value, alias, depth, out);
                }
            }
            Expr::Index { base, index, .. }
            | Expr::Binary {
                lhs: base,
                rhs: index,
                ..
            } => {
                self.expr_fnlit_store_pairs(base, alias, depth, out);
                self.expr_fnlit_store_pairs(index, alias, depth, out);
            }
            _ => {}
        }
    }

    fn calls_store_pairs(
        &self,
        calls: &[CallRef],
        alias: &HashMap<String, usize>,
        depth: usize,
        out: &mut Vec<(usize, usize)>,
    ) {
        if depth > 3 {
            return;
        }
        for cr in calls {
            let Expr::Ident { name, .. } = cr.callee else {
                continue;
            };
            let Some(decl) = self.fns.get(name).copied() else {
                continue;
            };
            for (di, si) in self.collect_param_store_pairs(decl, depth + 1) {
                let (Some(dst_arg), Some(src_arg)) = (cr.args.get(di), cr.args.get(si)) else {
                    continue;
                };
                let dst = dst_arg
                    .value
                    .root_ident()
                    .and_then(|r| alias.get(r).copied());
                let src = src_arg
                    .value
                    .root_ident()
                    .and_then(|r| alias.get(r).copied());
                if let (Some(dst), Some(src)) = (dst, src) {
                    if dst != src {
                        out.push((dst, src));
                    }
                }
            }
        }
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
        self.stmts_mutate_param(&block.stmts, param, depth)
    }

    fn stmts_mutate_param(&self, stmts: &[Stmt], param: &str, depth: usize) -> bool {
        for (i, s) in stmts.iter().enumerate() {
            if self.stmt_mutates_param(s, param, depth) {
                return true;
            }
            match s {
                Stmt::Local(l) => {
                    let mut shadowed = false;
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        let is_alias = walk::expr_path(val)
                            .map(|p| path_root(&p) == param)
                            .unwrap_or(false);
                        if is_alias {
                            if self.stmts_mutate_param(&stmts[i + 1..], name, depth) {
                                return true;
                            }
                        } else if name == param {
                            shadowed = true;
                        }
                    }
                    if shadowed {
                        return false;
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } if targets.len() == values.len() => {
                    let mut rebound_other = false;
                    for (t, val) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            let is_alias = walk::expr_path(val)
                                .map(|p| path_root(&p) == param)
                                .unwrap_or(false);
                            if is_alias {
                                if name != param
                                    && self.stmts_mutate_param(&stmts[i + 1..], name, depth)
                                {
                                    return true;
                                }
                            } else if name == param {
                                rebound_other = true;
                            }
                        }
                    }
                    if rebound_other {
                        return false;
                    }
                }
                _ => {}
            }
        }
        false
    }

    fn stmt_mutates_param(&self, stmt: &Stmt, param: &str, depth: usize) -> bool {
        match stmt {
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    if matches!(t.kind, PlaceKind::Index { .. }) && t.root() == param {
                        return true;
                    }
                }
                for v in values {
                    if self.expr_fnlit_mutates_param(v, param, depth) {
                        return true;
                    }
                }
            }
            Stmt::Local(l) => {
                for v in &l.values {
                    if self.expr_fnlit_mutates_param(v, param, depth) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        // calls in this statement (covers Expr/Send/Return/Defer/Task/...)
        let mut calls = Vec::new();
        walk::own_calls(stmt, &mut calls);
        for cr in &calls {
            if self.call_mutates_param(cr, param, depth) {
                return true;
            }
        }
        // nested bodies (if/for/match/select/block/managed)
        match stmt {
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    if self.stmt_mutates_param(i, param, depth) {
                        return true;
                    }
                }
                if self.block_mutates_param(then, param, depth) {
                    return true;
                }
                if let Some(o) = otherwise {
                    if self.stmt_mutates_param(o, param, depth) {
                        return true;
                    }
                }
            }
            Stmt::For { body, .. } | Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                if self.block_mutates_param(body, param, depth) {
                    return true;
                }
            }
            Stmt::Match { subject, arms, .. } => {
                let mut sroots = Vec::new();
                collect_ref_roots(subject, &mut sroots);
                let subject_reaches_param = sroots.iter().any(|r| r == param);
                for arm in arms {
                    if self.block_mutates_param(&arm.body, param, depth) {
                        return true;
                    }
                    if subject_reaches_param {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(bname),
                            ..
                        } = &arm.pattern
                        {
                            if self.block_mutates_param(&arm.body, bname, depth) {
                                return true;
                            }
                        }
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    if self.block_mutates_param(&arm.body, param, depth) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    /// A nested FnLit inside the callee body mutates `param` only if it
    /// captures it (a body declaring its own `param` shadows it — those
    /// references are the closure's fresh binding, not the caller's map).
    fn expr_fnlit_mutates_param(&self, e: &Expr, param: &str, depth: usize) -> bool {
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
            _ => false,
        }
    }

    /// Does this call site store through / delete `param`?
    fn call_mutates_param(&self, cr: &CallRef, param: &str, depth: usize) -> bool {
        let arg_root_is_param = |a: &hy_syntax::CallArg| {
            walk::expr_path(&a.value)
                .map(|p| path_root(&p) == param)
                .unwrap_or(false)
        };
        match cr.callee {
            Expr::Ident { name, .. } => {
                if name == "delete" {
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
    fn record_closure_mutations(&self, st: &mut FnState, name: &str, value: &Expr) {
        let Expr::FnLit {
            params,
            results,
            body,
            ..
        } = unwrap_parens(value)
        else {
            return;
        };
        let mut bound: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        bound.extend(results.iter().filter_map(|r| r.name.clone()));
        let roots = self.collect_mutation_roots(st, body, &bound);
        if !roots.is_empty() {
            st.closure_mutates
                .entry(name.to_string())
                .or_default()
                .extend(roots);
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
        let mut local_aliases: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        let mut local_shadows: HashSet<String> = bound.clone();
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
        for (stmt, _idx) in &flat.stmts {
            match stmt {
                Stmt::Local(l) => {
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        let resolved = walk::expr_path(val)
                            .and_then(|p| resolve(path_root(&p), &local_aliases, &local_shadows));
                        if let Some(r) = resolved {
                            local_shadows.remove(name.as_str());
                            local_aliases.insert(name.clone(), r);
                        } else {
                            local_aliases.remove(name.as_str());
                            local_shadows.insert(name.clone());
                        }
                    }
                }
                Stmt::Assign {
                    targets, values, ..
                } if targets.len() == values.len() => {
                    for (t, val) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(name) = &t.kind {
                            let resolved = walk::expr_path(val).and_then(|p| {
                                resolve(path_root(&p), &local_aliases, &local_shadows)
                            });
                            if let Some(r) = resolved {
                                local_shadows.remove(name.as_str());
                                local_aliases.insert(name.clone(), r);
                            } else if bound.contains(name.as_str()) {
                                local_aliases.remove(name.as_str());
                                local_shadows.insert(name.clone());
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
                    if matches!(t.kind, PlaceKind::Index { .. }) {
                        let raw = path_root(&walk::place_path(t).unwrap_or_default()).to_string();
                        if let Some(r) = resolve(&raw, &local_aliases, &local_shadows) {
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
                        if let Some(r) = resolve(&ir, &local_aliases, &local_shadows) {
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
                        if name == "delete" {
                            if let Some(first) = cr.args.first() {
                                if let Some(p) = walk::expr_path(&first.value) {
                                    if let Some(r) =
                                        resolve(path_root(&p), &local_aliases, &local_shadows)
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
                                let Some(p) = walk::expr_path(&a.value) else {
                                    continue;
                                };
                                let Some(r) =
                                    resolve(path_root(&p), &local_aliases, &local_shadows)
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
                        } else if let Some(existing) = st.closure_mutates.get(name.as_str()) {
                            // another local closure — union its mutation roots
                            out.extend(existing.iter().cloned());
                        } else {
                            // unknown callee — fail closed on path args
                            for a in cr.args {
                                if let Some(p) = walk::expr_path(&a.value) {
                                    if let Some(r) =
                                        resolve(path_root(&p), &local_aliases, &local_shadows)
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
                            if let Some(p) = walk::expr_path(&a.value) {
                                if let Some(r) =
                                    resolve(path_root(&p), &local_aliases, &local_shadows)
                                {
                                    out.insert(r);
                                }
                            }
                        }
                    }
                    _ => {
                        for a in cr.args {
                            if let Some(p) = walk::expr_path(&a.value) {
                                if let Some(r) =
                                    resolve(path_root(&p), &local_aliases, &local_shadows)
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
            if let Some(t) = &l.ty {
                self.check_zero_value_local(l, t);
            }
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
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            let tys = self.multi_value_tys(st, &l.values[0], l.names.len());
            let resolved: Vec<(String, TypeShape, Option<hy_syntax::TypeExpr>, Option<String>)> = l
                .names
                .iter()
                .zip(shapes)
                .zip(tys)
                .map(|((name, shape), ty)| {
                    let (shape, ty) = match (&l.ty, shape) {
                        (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                        (_, s) => (s, ty.or_else(|| l.ty.clone())),
                    };
                    let shadowed = if self.shape_may_share(&shape)
                        && self.value_reads_storage_of(st, &l.values[0], name)
                    {
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
                    (name.clone(), shape, ty, shadowed)
                })
                .collect();
            for (name, shape, ty, shadowed_origin) in resolved {
                let is_view = matches!(shape, TypeShape::BorrowHandle(_));
                self.define_binding(st, &name, shape.clone(), ty, immutable, managed);
                if is_view {
                    self.extend_call_loans_to_view(st, &l.values[0], &name, idx);
                }
                self.record_multi_value_share_and_alias(st, &name, &shape, &l.values[0]);
                if let Some(origin) = shadowed_origin {
                    if let Some(b) = st.env_lookup_mut(&name) {
                        if b.non_owned_origin.is_none() {
                            b.non_owned_origin = Some(origin);
                        }
                    }
                }
            }
        } else {
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                // Issue #148: constant initializer outside a fixed-width
                // declared type → HYB-E6002 (Go would refuse to build).
                if let Some(t) = &l.ty {
                    self.check_const_overflow_init(name, t, value);
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
                }
                // N1 for composite literals: `job{m: nil}` carries its own
                // type context (the struct def / map value / slice elem) —
                // the nested nil needs no ascription to reject (HYB-E4001).
                self.check_composite_nils(st, value);
                // Issue #146: typed receive `n int = <-ch` against `chan job`
                // used to accept and then fail `go build`. Compare the
                // ascription against the channel element when both resolve.
                if let Some(t) = &l.ty {
                    let want = self.shape_of_type(t);
                    self.check_recv_elem_against_target(
                        st,
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
                let inferred = self.shape_of_expr(st, value);
                let (shape, ty) = match (&l.ty, inferred) {
                    (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                    (_, inferred) => (inferred, self.ty_of_expr(st, value)),
                };
                let shadowed_const = self.shape_may_share(&shape)
                    && self.value_reads_storage_of(st, value, name)
                    && self
                        .const_write_root(
                            st,
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
                let moved_non_owned = self.moved_non_owned_origin_in_expr(st, value);
                if move_operand_ident(value).is_some()
                    || matches!(unwrap_parens(value), Expr::Move { .. })
                {
                    self.define_binding(st, name, shape, ty, immutable, managed);
                    if let Some(origin) = moved_non_owned {
                        if let Some(b) = st.env_lookup_mut(name) {
                            b.non_owned_origin = Some(origin);
                        }
                    }
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
                let reads_self = self.value_reads_storage_of(st, value, name);
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
                let prior_alias = if reads_self {
                    st.aliases.get(name).cloned()
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
                let is_view = matches!(shape, TypeShape::BorrowHandle(_));
                self.define_binding(st, name, shape, ty, immutable, managed);
                if let Some(origin) = &moved_non_owned {
                    if let Some(b) = st.env_lookup_mut(name) {
                        if b.non_owned_origin.is_none() {
                            b.non_owned_origin = Some(origin.clone());
                        }
                    }
                }
                if reads_self {
                    if let Some(pa) = prior_alias {
                        if pa != *name {
                            st.aliases.insert(name.clone(), pa);
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
                self.record_may_share(st, name, value);
                // alias recording: `m2 := agg` (single-parent link, the
                // friendlier "frozen alias" diagnostic) — and call results
                // that hand back argument storage (#38).
                self.record_alias(st, name, value);

                // #111 form 4: record the base paths a closure value's body
                // mutates, so a call inside a live slot-loan window counts
                // as an A3 insert/delete event.
                self.record_closure_mutations(st, name, value);
                // #141 (pr-140): the self-shadowing define keeps the SHADOWED
                // binding's non-owned origin — the RHS read the outer
                // storage, so the inner binding inherits O5.1's restriction
                // (move-at-boundary still rejects E3004).
                if let Some(origin) = shadowed_origin {
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
                let result = self.shape_of_expr(st, unwrap_parens(value));
                if !(self.shape_may_share(&result) || matches!(result, TypeShape::Unknown)) {
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
                via_arg || via_recv
            }
            _ => false,
        }
    }


    /// Does the binding `name` denote a value that a shallow Go copy
    /// would share mutable GC storage with? Includes file-level `const`
    /// composites (#125), which lower to package-level `var` in Go.
    fn binding_may_share(&self, st: &FnState, name: &str) -> bool {
        self.lookup_binding(st, name)
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
    pub(crate) fn record_may_share(&self, st: &mut FnState, name: &str, value: &Expr) {
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
                    if self.binding_may_share(st, &id) {
                        st.share_edge(name, &id);
                    }
                }
            }
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
                collect_addr_of_roots(value, &mut addr_roots);
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
                if d != s && self.binding_may_share(st, d) && self.binding_may_share(st, s) {
                    st.share_edge(d, s);
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
                let other_shape = self.shape_of_expr(st, unwrap_parens(other));
                if other_shape.is_reference_alias()
                    || matches!(other_shape, TypeShape::OwnHandle(_))
                {
                    let mut roots = Vec::new();
                    collect_ref_roots(value, &mut roots);
                    for src in roots {
                        if src == name {
                            continue;
                        }
                        let matched = self.lookup_binding(st, &src).map(|b| {
                            let ok = if matches!(other_shape, TypeShape::OwnHandle(_)) {
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
        }
    }

    /// Record O5.2 alias links for pointer borrows (`p := &stats`, `slot := &m[k]`)
    /// so moving `p` freezes the underlying mutable binding `stats` (#33).
    /// pr-312: the alias recording descends into NESTED address-of operands
    /// inside composite literals (`p := stats{cpu: &usage}`) via
    /// collect_addr_of_roots, not just the direct as_addr_of form.
    pub(crate) fn record_local_borrow_aliases(&self, st: &mut FnState, l: &LocalDecl) {
        if l.names.len() == l.values.len() {
            for (name, value) in l.names.iter().zip(l.values.iter()) {
                let inner = unwrap_parens(value);
                let is_borrow = as_addr_of(inner).is_some()
                    || contains_addr_of_in_literal(inner)
                    || matches!(inner, Expr::Ident { name: src, .. } if st.borrow_bindings.contains(src));
                if is_borrow {
                    st.borrow_bindings.insert(name.clone());
                } else {
                    st.borrow_bindings.remove(name);
                }
                if move_operand_ident(value).is_none() {
                    let shape = self.shape_of_expr(st, value);
                    if field_borrow_kind(&shape).is_none() && !matches!(shape, TypeShape::Unknown) {
                        continue;
                    }
                    let mut addr_roots = Vec::new();
                    collect_addr_of_roots(value, &mut addr_roots);
                    for src in addr_roots {
                        if src != *name {
                            if let Some(b) = self.lookup_binding(st, &src) {
                                if !b.shape.is_sendable_by_type() {
                                    st.aliases.insert(name.clone(), src);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Record O5.2 alias links for pointer-borrow rebinds (`p = &stats`).
    /// pr-312: nested address-of operands inside composite rebinds are
    /// tracked the same way as the define form.
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
                    let inner = unwrap_parens(v);
                    let is_borrow = as_addr_of(inner).is_some()
                        || contains_addr_of_in_literal(inner)
                        || matches!(inner, Expr::Ident { name: src, .. } if st.borrow_bindings.contains(src));
                    if is_borrow {
                        st.borrow_bindings.insert(name.clone());
                    } else {
                        st.borrow_bindings.remove(name);
                    }
                    if move_operand_ident(v).is_none() {
                        let shape = self.shape_of_expr(st, v);
                        if field_borrow_kind(&shape).is_none() && !matches!(shape, TypeShape::Unknown) {
                            continue;
                        }
                        let mut addr_roots = Vec::new();
                        collect_addr_of_roots(v, &mut addr_roots);
                        for src in addr_roots {
                            if src != *name {
                                if let Some(b) = self.lookup_binding(st, &src) {
                                    if !b.shape.is_sendable_by_type() {
                                        st.aliases.insert(name.clone(), src);
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
                                if self
                                    .lookup_binding(st, root)
                                    .map(|b| b.shape.is_handle())
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

    /// Record may-share and alias edges for one target `name` of a multi-value
    /// define or assignment (`h2, ok := alias_pair(h)`, `m, ok := buckets[k]`)
    /// using that target's resolved `shape` (#75).
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
        let mut roots = Vec::new();
        collect_ref_roots(value, &mut roots);
        for root in &roots {
            if root == name {
                continue;
            }
            if let Some(b) = self.lookup_binding(st, root) {
                if b.shape.is_handle()
                    && !self.shape_carries_handle(shape)
                    && !matches!(shape, TypeShape::Unknown)
                {
                    continue;
                }
                if self.shape_may_share(&b.shape) {
                    st.share_edge(name, root);
                }
            }
        }
        if shape.is_reference_alias() || matches!(shape, TypeShape::OwnHandle(_)) {
            for src in &roots {
                if src == name {
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
        if let Some(origin) = self.moved_non_owned_origin_in_expr(st, value) {
            if let Some(nb) = st.env_lookup_mut(name) {
                if nb.non_owned_origin.is_none() {
                    nb.non_owned_origin = Some(origin);
                }
            }
        }
    }

    /// Transfer `name`'s non-owned origin (O5.1) to any existing may-share
    /// peers and direct alias children before `name`'s share/alias edges are
    /// cleared by a shadowing define or non-self rebind (#75).
    fn transfer_non_owned_origin_to_peers_and_aliases(&self, st: &mut FnState, name: &str) {
        if let Some((orig, _)) = self.non_owned_origin_of(st, name) {
            let peer_origin = if orig.starts_with("a may-share peer of ") {
                orig.clone()
            } else {
                format!("a may-share peer of {}", orig)
            };
            let peers: Vec<String> = st.share_group(name).into_iter().collect();
            for peer in peers {
                if peer == name {
                    continue;
                }
                if let Some(pb) = st.env_lookup_mut(&peer) {
                    if pb.non_owned_origin.is_none() {
                        pb.non_owned_origin = Some(peer_origin.clone());
                    }
                }
            }
            let alias_children: Vec<String> = st
                .aliases
                .iter()
                .filter_map(|(k, v)| {
                    if v == name && k != name {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect();
            for child in alias_children {
                if let Some(cb) = st.env_lookup_mut(&child) {
                    if cb.non_owned_origin.is_none() {
                        cb.non_owned_origin = Some(orig.clone());
                    }
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
        // Issue #75: before a shadowing or same-scope define (`h2 := h;
        // if true { h := fresh; ... }` or `h2 := h; h, ok := fresh, true`)
        // clears `name`'s share/alias edges and replaces `name` in the lexical
        // environment, transfer `name`'s non-owned origin to any existing
        // may-share peers and direct alias children so they remain non-owned.
        self.transfer_non_owned_origin_to_peers_and_aliases(st, name);
        st.aliases.remove(name);
        st.share_clear(name);
        let non_owned_origin = if immutable && !shape.is_sendable_by_type() {
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
            if self.binding_may_share(st, r) {
                pre_share_peers.push(r.clone());
                pre_share_peers.extend(st.share_group(r));
            }
            if pre_alias_root.is_none()
                && self
                    .lookup_binding(st, r)
                    .map(|b| b.shape.is_reference_alias())
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
                if let PlaceKind::Ident(name) = &t.kind {
                    if let Some(path) = walk::addr_of_index_base_path(v) {
                        let binding = name.clone();
                        let last = st.last_use.get(&binding).copied().unwrap_or(idx).max(idx);
                        st.loans.push(Loan {
                            target: LoanTarget::MapSlot {
                                binding: binding.clone(),
                                path,
                            },
                            created: idx,
                            last,
                            span: v.span(),
                            desc: format!("`{} = {}`", binding, v.render()),
                        });
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
            self.rebind_assign_targets(st, targets, values, &is_const_write);
        }
    }

    pub(crate) fn process_closure_assign(
        &mut self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) {
        for v in values {
            self.mark_move_in_value(st, v, format!("the move-assign `{}`", v.render()));
        }
        if op == AssignOp::Assign {
            let no_const_mask = vec![false; targets.len()];
            self.rebind_assign_targets(st, targets, values, &no_const_mask);
        }
        self.record_assign_borrow_aliases(st, targets, op, values);
    }

    fn rebind_assign_targets(
        &mut self,
        st: &mut FnState,
        targets: &[Place],
        values: &[Expr],
        is_const_write: &[bool],
    ) {
        if values.len() == targets.len() {
            // Evaluate all RHS shapes, types, and alias sources before
            // mutating target bindings so parallel swaps/rebinds read the
            // pre-assignment environment.
            struct AssignRhsInfo {
                shape: TypeShape,
                ty: Option<hy_syntax::TypeExpr>,
                non_owned: Option<String>,
                reads_self: bool,
                shadowed_origin: Option<String>,
                prior_alias: Option<String>,
                prior_shares: Vec<String>,
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
                    let (reads_self, shadowed_origin, prior_alias, prior_shares) = match &t.kind {
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
                            (rs, so, pa, ps)
                        }
                        _ => (false, None, None, Vec::new()),
                    };
                    AssignRhsInfo {
                        shape,
                        ty,
                        non_owned,
                        reads_self,
                        shadowed_origin,
                        prior_alias,
                        prior_shares,
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
                    // Before clearing `name`'s non-owned origin and may-share
                    // edges on a non-self rebind (`h2 := h; h = fresh`), transfer
                    // `name`'s non-owned origin to any existing may-share peers
                    // and preserve clique connectivity among `name`'s former
                    // neighbors so they remain linked to each other.
                    if !info.reads_self {
                        self.transfer_non_owned_origin_to_peers_and_aliases(st, name);
                        let neighbors: Vec<String> = st
                            .may_share
                            .get(name)
                            .map(|s| s.iter().cloned().collect())
                            .unwrap_or_default();
                        for idx_a in 0..neighbors.len() {
                            for idx_b in (idx_a + 1)..neighbors.len() {
                                st.share_edge(&neighbors[idx_a], &neighbors[idx_b]);
                            }
                        }
                    }

                    if let Some(b) = st.env_lookup_mut(name) {
                        if !matches!(info.shape, TypeShape::Unknown) || b.ty.is_none() {
                            b.shape = info.shape;
                        }
                        if info.ty.is_some() || b.ty.is_none() {
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
                        if let Some(pa) = info.prior_alias {
                            if &pa != name {
                                st.aliases.insert(name.clone(), pa);
                            }
                        }
                        for peer in info.prior_shares {
                            st.share_edge(name, &peer);
                        }
                    }
                    self.record_may_share(st, name, v);
                    // rebind may create a fresh alias — ident copies AND
                    // call results that hand back argument storage (#38)
                    self.record_alias(st, name, v);
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
                    self.record_closure_mutations(st, name, v);
                } else {
                    let root = t.root();
                    if !root.is_empty() && self.binding_may_share(st, root) {
                        self.record_may_share(st, root, v);
                    }
                }
            }
        } else if values.len() == 1 && targets.len() > 1 {
            let shapes = self.multi_value_shapes(st, &values[0], targets.len());
            let tys = self.multi_value_tys(st, &values[0], targets.len());
            for (i, ((t, shape), ty)) in targets.iter().zip(shapes).zip(tys).enumerate() {
                if is_const_write.get(i).copied().unwrap_or(false) {
                    continue;
                }
                if let PlaceKind::Ident(name) = &t.kind {
                    let reads_self = self.value_reads_storage_of(st, &values[0], name);
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
                    let prior_alias = if reads_self {
                        st.aliases.get(name).cloned()
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
                    if !reads_self {
                        self.transfer_non_owned_origin_to_peers_and_aliases(st, name);
                    }
                    if let Some(b) = st.env_lookup_mut(name) {
                        if !matches!(shape, TypeShape::Unknown) || b.ty.is_none() {
                            b.shape = shape.clone();
                        }
                        if ty.is_some() || b.ty.is_none() {
                            b.ty = ty;
                        }
                        b.moved = None;
                        b.non_owned_origin = None;
                    }
                    st.aliases.remove(name);
                    st.share_clear(name);
                    if reads_self {
                        if let Some(pa) = prior_alias {
                            if &pa != name {
                                st.aliases.insert(name.clone(), pa);
                            }
                        }
                        for peer in prior_shares {
                            st.share_edge(name, &peer);
                        }
                    }
                    self.record_multi_value_share_and_alias(st, name, &shape, &values[0]);
                    if let Some(origin) = shadowed_origin {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin);
                            }
                        }
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
    fn binding_const_root(&self, st: &FnState, name: &str) -> Option<String> {
        if let Some(b) = self.lookup_binding(st, name) {
            if b.immutable {
                return Some(name.to_string());
            }
            if let Some(origin) = &b.non_owned_origin {
                if let Some(idx) = origin.find("`const ") {
                    let rest = &origin[idx + 7..];
                    if let Some(end) = rest.find('`') {
                        return Some(rest[..end].to_string());
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
                                self.define_binding(&mut cst, n, shape, None, false, false);
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
                }
            }

            match stmt {
                Stmt::Local(l) => {
                    self.process_closure_const_local(&mut cst, l);
                }
                Stmt::Assign {
                    targets,
                    op,
                    values,
                    ..
                } => {
                    let is_const_write: Vec<bool> = targets
                        .iter()
                        .map(|t| match self.const_write_root(&cst, t) {
                            Some(name) => {
                                if check_writes {
                                    self.emit_const_write_error(&name, t.span);
                                }
                                true
                            }
                            None => false,
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
                                self.record_alias(&mut cst, name, v);
                            }
                        }
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
                            self.define_binding(&mut cst, name, shape, None, false, false);
                            if !is_chan {
                                let mut roots = Vec::new();
                                collect_ref_roots(expr, &mut roots);
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
                        self.define_binding(&mut cst, var, TypeShape::Primitive, None, false, false);
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
            self.define_binding(st, &p.name, shape, Some(p.ty.clone()), false, false);
        }
        for r in results {
            if let Some(name) = &r.name {
                if name != "_" {
                    let shape = self.shape_of_type(&r.ty);
                    self.define_binding(st, name, shape, Some(r.ty.clone()), false, false);
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
        for (name, value) in l.names.iter().zip(l.values.iter()) {
            let inferred = self.shape_of_expr(st, value);
            let (shape, ty) = match (&l.ty, inferred) {
                (Some(t), TypeShape::Unknown) => (self.shape_of_type(t), Some(t.clone())),
                (_, inferred) => (inferred, self.ty_of_expr(st, value).or_else(|| l.ty.clone())),
            };
            let immutable = l.kind == LocalKind::Const;
            let shadowed_const = self.shape_may_share(&shape)
                && self.value_reads_storage_of(st, value, name)
                && self
                    .const_write_root(
                        st,
                        &Place {
                            kind: PlaceKind::Ident(name.clone()),
                            span: value.span(),
                        },
                    )
                    .is_some();
            self.define_binding(st, name, shape, ty, immutable || shadowed_const, false);
            if move_operand_ident(value).is_none() {
                self.record_may_share(st, name, value);
                self.record_alias(st, name, value);
            }
        }
    }

    fn check_delete_const(&mut self, st: &FnState, cr: &CallRef, declared: &HashSet<String>) {
        let Expr::Ident { name, .. } = crate::model::unwrap_parens(cr.callee) else {
            return;
        };
        if name.as_str() != "delete" && name.as_str() != "copy" {
            return;
        }
        let Some(first) = cr.args.first() else {
            return;
        };
        if let Some(hit) = self.const_arg_root(st, &first.value, declared) {
            self.emit_const_write_error(&hit, first.value.span());
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
            let root = addr_op.root_ident()?;
            if declared.contains(root) {
                return None;
            }
            let place = Place {
                kind: PlaceKind::Ident(root.to_string()),
                span: expr.span(),
            };
            return self.const_write_root(st, &place);
        }
        let arg_shape = self.shape_of_expr(st, inner);
        if !(self.shape_may_share(&arg_shape) || matches!(arg_shape, TypeShape::Unknown)) {
            return None;
        }
        let mut roots = Vec::new();
        collect_ref_roots(inner, &mut roots);
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
    fn check_call_const_writes(&mut self, st: &FnState, cr: &CallRef, declared: &HashSet<String>) {
        let callee = crate::model::unwrap_parens(cr.callee);
        if let Expr::Ident { name, .. } = callee {
            if name == "delete" || name == "copy" || A3_PURE_FNS.contains(&name.as_str()) {
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
            }
        }
        for (i, a) in cr.args.iter().enumerate() {
            let Some(const_name) = self.const_arg_root(st, &a.value, declared) else {
                continue;
            };
            let mutates = match callee {
                Expr::Ident { name, .. } => {
                    if declared.contains(name) {
                        true
                    } else if let Some(decl) = self.fns.get(name) {
                        match decl.params.get(i) {
                            Some(p) => self.fn_mutates_const_param(decl, &p.name, 0),
                            None => true,
                        }
                    } else if self
                        .lookup_binding(st, name)
                        .map(|b| matches!(b.shape, TypeShape::Closure))
                        .unwrap_or(false)
                    {
                        true
                    } else {
                        false
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
                        false
                    } else if matches!(self.shape_of_expr(st, callee), TypeShape::Closure) {
                        true
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
                                Some(p) => self.fn_mutates_const_param(decl, &p.name, 0),
                                None => true,
                            }
                        } else {
                            false
                        }
                    }
                }
                Expr::FnLit { params, body, .. } => match params.get(i) {
                    Some(p) => {
                        let mut aliases = HashSet::new();
                        aliases.insert(p.name.clone());
                        self.block_mutates_const_param(body, &mut aliases, 0)
                    }
                    None => true,
                },
                _ => false,
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
            _ => {}
        }
        let mut calls = Vec::new();
        walk::own_calls(stmt, &mut calls);
        for cr in &calls {
            if self.call_mutates_const_param(cr, aliases, depth) {
                return true;
            }
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
                if let Some(o) = otherwise {
                    let mut else_aliases = branch_aliases;
                    if self.stmt_mutates_const_param(o, &mut else_aliases, depth) {
                        return true;
                    }
                }
            }
            Stmt::For { body, .. }
            | Stmt::Managed { body, .. }
            | Stmt::Block { block: body, .. } => {
                let mut body_aliases = aliases.clone();
                if self.block_mutates_const_param(body, &mut body_aliases, depth) {
                    return true;
                }
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    let mut arm_aliases = aliases.clone();
                    if self.block_mutates_const_param(&arm.body, &mut arm_aliases, depth) {
                        return true;
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    if let hy_syntax::SelectArmKind::Send { value, .. } = &arm.kind {
                        if Self::expr_reaches_const_aliases(value, aliases) {
                            return true;
                        }
                    }
                    let mut arm_aliases = aliases.clone();
                    if self.block_mutates_const_param(&arm.body, &mut arm_aliases, depth) {
                        return true;
                    }
                }
            }
            _ => {}
        }
        false
    }

    fn expr_fnlit_mutates_const_param(
        &self,
        e: &Expr,
        aliases: &HashSet<String>,
        depth: usize,
    ) -> bool {
        match e {
            Expr::FnLit { params, body, .. } => {
                let mut inner_aliases = aliases.clone();
                for p in params {
                    inner_aliases.remove(&p.name);
                }
                if inner_aliases.is_empty() {
                    false
                } else {
                    self.block_mutates_const_param(body, &mut inner_aliases, depth)
                }
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. } => {
                self.expr_fnlit_mutates_const_param(inner, aliases, depth)
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
                if name == "delete" || name == "copy" {
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
                    && (matching.is_empty()
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
            _ => cr.args.iter().any(&arg_reaches_alias),
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
            if as_addr_of(v).is_some() || contains_addr_of_in_literal(v) || is_bound_borrow {
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
// #109: closure-capture ident collection (may-share edges at definition)
// ---------------------------------------------------------------------------

/// All identifier uses inside a closure body that may resolve to an
/// enclosing binding. `bound` holds names shadowed by the closure's own
/// parameters (and nested closures'), so those idents are not captures.
/// Locals defined inside the body are deliberately NOT excluded: a name
/// that also exists outside resolves to the outer binding on other paths,
/// and an extra may-share edge is conservative in the safe direction.
fn collect_capture_idents_block(
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
                    hy_syntax::SelectArmKind::Send { chan, value } => {
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
        let non_owned_origin = if !shape.is_sendable_by_type() {
            Some(format!("file-level `const {}`", c.name))
        } else {
            None
        };
        Binding {
            shape,
            ty,
            immutable: true, // class 4: const values are immutable-by-type
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
}

