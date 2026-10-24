//! closures.rs — the closure-boundary pass (Finding 2, human directive
//! 2026-10-13).
//!
//! Tier 0's flat walk (walk.rs) does not descend into closure (FnLit)
//! bodies: captures, loan windows, freeze and alias tracking stay outside
//! (the A5 perimeter — a closure handed to foreign Go code can run at any
//! time, and binding-level analysis cannot see Go's dispatch). That boundary
//! remains, and it is documented, NOT silent.
//!
//! What changed: a `task`/`send`/select-send STATEMENT inside a closure body
//! is still a concurrency boundary in the emitted Go (a goroutine spawn, a
//! channel operation) — the checker owns those sites wherever they appear.
//! This pass finds every closure body under a function's statements and
//! checks exactly those boundary statements, fail-closed (O4.9):
//!
//!   * the closure's params are ascribed (the fn-type syntax requires it),
//!     so they classify normally;
//!   * captures resolve against the enclosing function's environment —
//!     Tier-0 binding-level analysis shares the env (a documented
//!     over-approximation: a capture's SHAPE decides, not lexical order);
//!   * a capture whose shape is `Unknown` is UNPROVEN at the boundary —
//!     HYB-E3005 unless the site sits in a `managed` block, exactly like
//!     the flat walk;
//!   * everything else inside the closure (assignments, calls, captures of
//!     mutable state without a boundary statement) stays outside the checked
//!     subset — corpus 10's `hits++` handler shape is the standing example
//!     (A5, QUESTIONS.md Q1/OQ1: pre-registered ACCEPT, contested openly).

use crate::model::{Binding, Checker, FnState, TypeShape};
use hy_syntax::ForKind::*;
use hy_syntax::{Block, Expr, LocalKind, SelectArmKind, Stmt};

/// Check the boundary statements of every closure body reachable from
/// `body` (one env per closure, captures included). Called once per
/// function, after the flat Pass B loop — the enclosing env is complete.
pub(crate) fn check_closure_boundaries(ck: &mut Checker<'_>, st: &FnState, body: &Block) {
    check_deferred_uses(ck, st, body);
    for (params, results, body, pos) in collect_fnlits(body) {
        let mut scoped_st;
        let st_ref = if let Some((s_env, s_aliases, s_share)) = st.closure_scoped_envs.get(&pos) {
            scoped_st = st.clone();
            for (k, v) in s_env {
                scoped_st.env.insert(k.clone(), v.clone());
            }
            for (k, v) in s_aliases {
                scoped_st.aliases.insert(k.clone(), v.clone());
            }
            for (k, vs) in s_share {
                scoped_st
                    .may_share
                    .entry(k.clone())
                    .or_default()
                    .extend(vs.iter().cloned());
            }
            &scoped_st
        } else {
            st
        };
        check_one_closure(ck, st_ref, params, results, body);
    }
}

struct NestedClosure<'a> {
    params: &'a [hy_syntax::Param],
    results: &'a [hy_syntax::FnResult],
    body: &'a Block,
    saved_scopes: Vec<std::collections::HashMap<String, Binding>>,
    saved_aliases: std::collections::HashMap<String, String>,
    saved_share: std::collections::HashMap<String, std::collections::HashSet<String>>,
    overwritten: std::collections::HashSet<(usize, String)>,
}

fn push_nested_closure<'a>(
    st: &FnState,
    params: &'a [hy_syntax::Param],
    results: &'a [hy_syntax::FnResult],
    body: &'a Block,
    nested: &mut Vec<NestedClosure<'a>>,
) {
    nested.push(NestedClosure {
        params,
        results,
        body,
        saved_scopes: st.scopes.clone(),
        saved_aliases: st.aliases.clone(),
        saved_share: st.may_share.clone(),
        overwritten: std::collections::HashSet::new(),
    });
}

fn mark_nested_overwritten(st: &FnState, name: &str, nested: &mut [NestedClosure<'_>]) {
    if let Some(top) = st.scopes.last() {
        if top.contains_key(name) {
            let depth = st.scopes.len() - 1;
            for nc in nested.iter_mut() {
                if depth < nc.saved_scopes.len() && nc.saved_scopes[depth].contains_key(name) {
                    nc.overwritten.insert((depth, name.to_string()));
                }
            }
        }
    }
}

fn is_overwritten_in_nc(st: &FnState, nc: &NestedClosure<'_>, name: &str) -> bool {
    match st.lookup_depth(name) {
        Some(0) | None => false,
        Some(d) => nc.overwritten.contains(&(d - 1, name.to_string())),
    }
}

fn sync_nested_moves(st: &FnState, nested: &mut [NestedClosure<'_>]) {
    for nc in nested.iter_mut() {
        for (depth, scope) in st.scopes.iter().enumerate() {
            if depth < nc.saved_scopes.len() {
                for (k, b) in scope {
                    if !nc.overwritten.contains(&(depth, k.clone())) {
                        if let Some(sb) = nc.saved_scopes[depth].get_mut(k) {
                            if sb.moved.is_none() && b.moved.is_some() {
                                sb.moved = b.moved.clone();
                            }
                        }
                    }
                }
            }
        }
        for (k, v) in &st.aliases {
            if !is_overwritten_in_nc(st, nc, k) && !is_overwritten_in_nc(st, nc, v) {
                nc.saved_aliases.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
        for (k, vs) in &st.may_share {
            if !is_overwritten_in_nc(st, nc, k) {
                for v in vs {
                    if !is_overwritten_in_nc(st, nc, v) {
                        nc.saved_share.entry(k.clone()).or_default().insert(v.clone());
                        nc.saved_share.entry(v.clone()).or_default().insert(k.clone());
                    }
                }
            }
        }
        let mut active_moves: Vec<(String, crate::model::MoveInfo)> = Vec::new();
        for (k, b) in &st.env {
            if let Some(mv) = &b.moved {
                active_moves.push((k.clone(), mv.clone()));
            }
        }
        for (depth, scope) in st.scopes.iter().enumerate() {
            for (k, b) in scope {
                if !nc.overwritten.contains(&(depth, k.clone())) {
                    if let Some(mv) = &b.moved {
                        active_moves.push((k.clone(), mv.clone()));
                    }
                }
            }
        }
        for (moved_name, mv) in active_moves {
            let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
            let mut stack = vec![moved_name];
            while let Some(cur) = stack.pop() {
                if !seen.insert(cur.clone()) {
                    continue;
                }
                if let Some(parent) = nc.saved_aliases.get(&cur) {
                    stack.push(parent.clone());
                }
                for (child, parent) in &nc.saved_aliases {
                    if parent == &cur {
                        stack.push(child.clone());
                    }
                }
                if let Some(peers) = nc.saved_share.get(&cur) {
                    for p in peers {
                        stack.push(p.clone());
                    }
                }
            }
            for member in seen {
                for scope in nc.saved_scopes.iter_mut().rev() {
                    if let Some(sb) = scope.get_mut(&member) {
                        if sb.moved.is_none() {
                            sb.moved = Some(mv.clone());
                        }
                        break;
                    }
                }
            }
        }
    }
}

fn pop_scope_and_sync(st: &mut FnState, nested: &mut [NestedClosure<'_>]) {
    sync_nested_moves(st, nested);
    st.scope_pop();
    sync_nested_moves(st, nested);
}

fn check_one_closure(
    ck: &mut Checker<'_>,
    st: &FnState,
    params: &[hy_syntax::Param],
    results: &[hy_syntax::FnResult],
    body: &Block,
) {
    // env: captures (the enclosing bindings, including any active scopes from
    // an enclosing closure) + the closure's ascribed params and named result
    // bindings.
    let mut env = st.env.clone();
    for scope in &st.scopes {
        for (k, v) in scope {
            env.insert(k.clone(), v.clone());
        }
    }
    let mut cst = FnState {
        env,
        aliases: st.aliases.clone(),
        may_share: st.may_share.clone(),
        ..FnState::default()
    };
    for (k, b) in cst.env.iter_mut() {
        if b.non_owned_origin.is_none() {
            b.non_owned_origin = Some(format!(
                "captured outer binding `{}` inside a closure (the enclosing scope retains `{}` and the closure may run multiple times)",
                k, k
            ));
        }
    }
    for p in params {
        let shape = ck.shape_of_type(&p.ty);
        cst.env.insert(
            p.name.clone(),
            Binding {
                shape,
                ty: Some(p.ty.clone()),
                immutable: false,
                is_let: false,
                managed_taint: false,
                moved: None,
                slot_loan: false,
                non_owned_origin: Some(format!(
                    "closure parameter `{}` (the caller retains the argument across calls)",
                    p.name
                )),
            },
        );
    }
    for r in results {
        let shape = ck.shape_of_type(&r.ty);
        if let Some(name) = &r.name {
            if name != "_" {
                cst.env.insert(
                    name.clone(),
                    Binding {
                        shape: shape.clone(),
                        ty: Some(r.ty.clone()),
                        immutable: false,
                        is_let: false,
                        managed_taint: false,
                        moved: None,
                        non_owned_origin: None,
                        slot_loan: false,
                    },
                );
                cst.named_results.push(name.clone());
            }
        }
        cst.result_shapes.push(shape);
    }
    let mut nested = Vec::new();
    cst.scope_push();
    check_closure_slot_loans(ck, &mut cst, body);
    check_body_boundaries(ck, &mut cst, body, false, &mut nested);
    sync_nested_moves(&cst, &mut nested);
    for nc in nested {
        let mut snap = cst.clone();
        snap.scopes = nc.saved_scopes;
        // A nested closure checks against ITS OWN named results: reset the
        // enclosing closure's seeded state, check_one_closure re-seeds from
        // nresults (a bare return inside the nested closure must not record
        // uses for the enclosing closure's result names).
        snap.named_results = Vec::new();
        snap.result_shapes = Vec::new();
        for (k, v) in nc.saved_aliases {
            snap.aliases.insert(k, v);
        }
        for (k, vs) in nc.saved_share {
            snap.may_share.entry(k).or_default().extend(vs);
        }
        let moved_entries: Vec<(String, crate::model::MoveInfo)> = snap
            .env
            .iter()
            .chain(snap.scopes.iter().flat_map(|s| s.iter()))
            .filter_map(|(k, b)| b.moved.as_ref().map(|mv| (k.clone(), mv.clone())))
            .collect();
        for (moved_name, mv) in moved_entries {
            let root = crate::freeze::resolve_alias(&snap, &moved_name);
            let mut members = vec![moved_name, root.clone()];
            for a in snap.aliases.keys() {
                if crate::freeze::resolve_alias(&snap, a) == root {
                    members.push(a.clone());
                }
            }
            let seeded = members.clone();
            for m in seeded {
                members.extend(snap.share_group(&m));
            }
            for m in members {
                if let Some(b) = snap.env_lookup_mut(&m) {
                    if b.moved.is_none() {
                        b.moved = Some(mv.clone());
                    }
                }
            }
        }
        check_one_closure(ck, &snap, nc.params, nc.results, nc.body);
    }
    check_deferred_uses(ck, &cst, body);
    cst.scope_pop();
}

/// Walk one closure body recursively, checking boundary statements only.
/// Locals/select-recvs define bindings so a later boundary classifies them;
/// moves inside the closure mark the closure-local view (a closure can run
/// more than once — the enclosing function's env is untouched).
fn check_body_boundaries<'a>(
    ck: &mut Checker<'_>,
    st: &mut FnState,
    block: &'a Block,
    managed: bool,
    nested: &mut Vec<NestedClosure<'a>>,
) {
    for stmt in &block.stmts {
        check_stmt_boundaries(ck, st, stmt, managed, nested);
    }
}

fn collect_expr_direct_fnlits<'a>(
    expr: &'a Expr,
    st: &FnState,
    nested: &mut Vec<NestedClosure<'a>>,
) {
    let mut flits = Vec::new();
    crate::walk::expr_fnlits(expr, &mut flits);
    for flit in flits {
        if let Expr::FnLit {
            params,
            results,
            body,
            ..
        } = crate::model::unwrap_parens(flit)
        {
            push_nested_closure(st, params, results, body, nested);
        }
    }
}

fn collect_stmt_direct_fnlits<'a>(
    stmt: &'a Stmt,
    st: &FnState,
    nested: &mut Vec<NestedClosure<'a>>,
) {
    let mut flits = Vec::new();
    crate::walk::own_fnlits(stmt, &mut flits);
    if let Stmt::Assign { targets, .. } = stmt {
        for t in targets {
            collect_place_direct_fnlits(t, &mut flits);
        }
    }
    for flit in flits {
        if let Expr::FnLit {
            params,
            results,
            body,
            ..
        } = crate::model::unwrap_parens(flit)
        {
            push_nested_closure(st, params, results, body, nested);
        }
    }
}

fn collect_place_direct_fnlits<'a>(place: &'a hy_syntax::Place, out: &mut Vec<&'a Expr>) {
    match &place.kind {
        hy_syntax::PlaceKind::Ident(_) => {}
        hy_syntax::PlaceKind::Index { base, index } => {
            collect_place_direct_fnlits(base, out);
            crate::walk::expr_fnlits(index, out);
        }
        hy_syntax::PlaceKind::Field { base, .. } | hy_syntax::PlaceKind::AddrOf { base } => {
            collect_place_direct_fnlits(base, out);
        }
    }
}

fn check_stmt_own_uses(ck: &mut Checker<'_>, st: &mut FnState, stmt: &Stmt, managed: bool) {
    let mut uses = Vec::new();
    crate::walk::own_uses_with_results(stmt, &st.named_results, &mut uses);
    if !managed {
        for u in &uses {
            if u.via_move && !st.scopes.iter().any(|s| s.contains_key(&u.name)) {
                if let Some(b) = st.env.get_mut(&u.name) {
                    if b.moved.is_none()
                        && !b.shape.is_sendable_by_type()
                        && !b.immutable
                        && !b.managed_taint
                    {
                        b.moved = Some(crate::model::MoveInfo {
                            span: u.span,
                            kind: crate::model::MoveKind::Moved,
                            desc: format!(
                                "closure capture of `{}` (the enclosing scope retains `{}`)",
                                u.name, u.name
                            ),
                            moved_name: u.name.clone(),
                        });
                    }
                }
            }
        }
    }
    ck.check_uses(st, &uses);
}

fn check_stmt_boundaries<'a>(ck: &mut Checker<'_>, st: &mut FnState, stmt: &'a Stmt, managed: bool, nested: &mut Vec<NestedClosure<'a>>) {
    // Check uses on every statement in the closure BEFORE any new defines or
    // move transfers on this statement take effect (#29). For `Stmt::If`,
    // init executes BEFORE `cond`, so `cond`'s uses are checked after `init`
    // (#306 eval-order alignment, carried into the nested-closure walk).
    if !matches!(stmt, Stmt::If { .. }) {
        check_stmt_own_uses(ck, st, stmt, managed);
    }

    // Issue #411: closure bodies sit outside the Tier-0 flat walk, so the
    // range-iterable rule (HYB-E2009/E3005) must fire here too — the same
    // A5 perimeter discipline as the send-side checks below.
    ck.check_stmt_range_iterable(st, stmt);

    // Issue #409: closure bodies sit outside the Tier-0 flat walk, so the
    // receive-operand rule (HYB-E2008/E3005) must fire here too — the same
    // perimeter discipline as the send-side checks below. Pure type
    // classification: no use/define ordering concerns.
    ck.check_stmt_recv_operands(st, stmt);

    // Issue #380: closure bodies sit outside the Tier-0 flat walk, so T23
    // multi-value binding arity checks (HYB-E2002) must fire here too.
    ck.check_stmt_multi_value(st, stmt);
    // #238: `copy(dst, src)` inside a closure body duplicates reference-bearing
    // slice elements, so `dst` and `src` share mutable GC storage and a later
    // `move` of either inside the closure must freeze the other (O5.2) and
    // inherit non-owned capture/parameter origin (O5.1 / O5.4).
    let mut calls = Vec::new();
    if let Stmt::For {
        kind: Iterate { expr, .. },
        ..
    } = stmt
    {
        crate::walk::expr_calls(expr, &mut calls);
    } else {
        crate::walk::own_calls(stmt, &mut calls);
    }
    for cr in &calls {
        if let Expr::Ident { name, .. } = crate::model::unwrap_parens(cr.callee) {
            if name == "copy" {
                ck.record_copy_may_share(st, cr);
            }
        }
        ck.record_callee_param_stores(st, cr);
    }
    // Issue #288 integration: closure bodies sit outside the Tier-0 flat
    // walk, where builtins.rs owns the HYB-E2001 defer-operand rule — so a
    // `defer 3` one scope in passed main with 0 diagnostics and failed late
    // in the emitter. One owner (builtins.rs), wired here for the A5
    // perimeter. Pure shape classification: no use/define ordering concerns.
    ck.check_stmt_defer_call(stmt);

    match stmt {
        Stmt::Local(l) => {
            if let Some(t) = &l.ty {
                let shape = ck.shape_of_type(t);
                for (name, value) in l.names.iter().zip(l.values.iter()) {
                    if Checker::is_nil_literal(value)
                        && !matches!(shape, TypeShape::Unknown)
                        && !Checker::is_nilable_shape(&shape)
                    {
                        ck.emit_nil_type_error_text(
                            &shape,
                            t.render(),
                            value.span(),
                            &format!("the declared type of `{}`", name),
                        );
                    }
                }
            } else if l.kind != LocalKind::Const {
                // N1, untyped leg (HYB-E4003): a closure-body `var x = nil` /
                // `x := nil` / `let x = nil` — same gap as the flat walk's
                // process_local (no ascription → Unknown shape → the emitter
                // emits `var x = nil`, which Go rejects). Local
                // `const c = nil` is the E6003 constant-expression guard
                // (issue #640) — different locus, do not double-fire.
                for (name, value) in l.names.iter().zip(l.values.iter()) {
                    if Checker::is_nil_literal(value) {
                        ck.emit_untyped_nil_decl_error(name, value.span());
                    }
                }
            }
            for v in &l.values {
                ck.check_composite_nils(st, v);
                ck.mark_move_in_value(st, v, format!("the move-define `{}`", v.render()));
            }
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
            // T22 (issue #12 parity with the flat pass): only `const` is
            // DEEPLY immutable; `let` is a SHALLOW immutable binding, so it
            // must NOT take the class-4 sendability shortcut.
            let immutable = l.kind == LocalKind::Const;
            let is_let = l.kind == LocalKind::Let;
            if l.values.is_empty() {
                if let Some(t) = &l.ty {
                    ck.check_zero_value_local(l, t);
                }
                let shape = l
                    .ty
                    .as_ref()
                    .map(|t| ck.shape_of_type(t))
                    .unwrap_or(TypeShape::Unknown);
                let ty = l.ty.clone();
                for name in &l.names {
                    mark_nested_overwritten(st, name, nested);
                    ck.define_binding(st, name, shape.clone(), ty.clone(), immutable, managed, is_let);
                }
            } else if l.names.len() == l.values.len() {
                let pre_st = st.clone();
                let mut overwritten = Vec::new();
                for (name, value) in l.names.iter().zip(l.values.iter()) {
                    overwritten.push(name.clone());
                    let moved_non_owned = crate::model::move_operand_ident(value)
                        .and_then(|src| ck.lookup_binding(&pre_st, &src))
                        .and_then(|b| b.non_owned_origin.clone())
                        .or_else(|| ck.moved_non_owned_origin_in_expr(&pre_st, value));
                    // ascription fills gaps (O4.9); inference keeps priority
                    // when it resolves (boundary handles stay precise)
                    let inferred = ck.shape_of_expr(&pre_st, value);
                    let (shape, ty) = match (&l.ty, inferred) {
                        (Some(t), TypeShape::Unknown) => (ck.shape_of_type(t), Some(t.clone())),
                        (_, inferred) => (
                            inferred,
                            ck.ty_of_expr(&pre_st, value).or_else(|| l.ty.clone()),
                        ),
                    };
                    let is_fresh_copy = ck.call_returns_fresh_copy(&pre_st, value);
                    if crate::model::move_operand_ident(value).is_some()
                        || matches!(crate::model::unwrap_parens(value), Expr::Move { .. })
                    {
                        mark_nested_overwritten(st, name, nested);
                        ck.define_binding(st, name, shape, ty, immutable, managed, is_let);
                        if let Some(origin) = moved_non_owned {
                            if let Some(b) = st.env_lookup_mut(name) {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                        ck.record_move_const_and_a3_loans(st, name, value, 0);
                        continue;
                    }
                    let reads_self = !is_fresh_copy && ck.value_reads_storage_of(&pre_st, value, name);
                    if !reads_self {
                        mark_nested_overwritten(st, name, nested);
                    }
                    let shadowed_origin = if ck.shape_may_share(&shape) && reads_self {
                        ck.non_owned_origin_of(&pre_st, name).map(|(o, via_share)| {
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
                        pre_st.aliases.get(name).cloned()
                    } else {
                        None
                    };
                    let prior_children: Vec<String> = if reads_self {
                        pre_st
                            .aliases
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
                        std::collections::HashSet::new()
                    };
                    let outer_depth = if pre_st
                        .scopes
                        .last()
                        .is_some_and(|top| !top.contains_key(name))
                    {
                        pre_st.lookup_depth(name)
                    } else {
                        None
                    };
                    ck.define_binding(st, name, shape, ty, immutable, managed, is_let);
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
                            if pa != *name {
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
                    // issue #137: track may_share edges and aliases inside closures
                    // so local aliases of non-owned captured bindings inherit origin
                    // and cannot bypass O5.1 / O5.4 move restrictions.
                    ck.record_storage_from_snapshot(st, &pre_st, name, value, &overwritten, true, false);
                    ck.record_move_const_and_a3_loans(st, name, value, 0);

                    ck.record_closure_mutations(st, name, value);
                    if let Some(origin) = shadowed_origin.or(moved_non_owned) {
                        if let Some(b) = st.env_lookup_mut(name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                    }
                }
                ck.record_local_borrow_aliases(st, l);
            } else {
                // multi-name define from one value (`v, ok := m[k]`, `a, b := f()`)
                let moved_non_owned = l
                    .values
                    .first()
                    .and_then(|v| ck.moved_non_owned_origin_in_expr(st, v));
                let (shapes, tys) = match l.values.first() {
                    Some(v) => (
                        ck.multi_value_shapes(st, v, l.names.len()),
                        ck.multi_value_tys(st, v, l.names.len()),
                    ),
                    None => (
                        vec![TypeShape::Unknown; l.names.len()],
                        vec![None; l.names.len()],
                    ),
                };
                let val_opt = l.values.first();
                let resolved: Vec<(
                    String,
                    TypeShape,
                    Option<hy_syntax::TypeExpr>,
                    bool,
                    Option<String>,
                    Option<String>,
                    Vec<String>,
                )> = l
                    .names
                    .iter()
                    .zip(shapes)
                    .zip(tys)
                    .map(|((name, shape), ty)| {
                        let reads_self = val_opt
                            .map(|v| ck.value_reads_storage_of(st, v, name))
                            .unwrap_or(false);
                        let shadowed_origin = if ck.shape_may_share(&shape) && reads_self {
                            ck.non_owned_origin_of(st, name).map(|(o, via_share)| {
                                if via_share && !o.starts_with("a may-share peer of ") {
                                    format!("a may-share peer of {}", o)
                                } else {
                                    o
                                }
                            })
                        } else {
                            None
                        };
                        let outer_exists = ck.lookup_binding(st, name).is_some();
                        let prior_alias = if reads_self {
                            st.aliases.get(name).cloned().or_else(|| {
                                if ck.shape_may_share(&shape)
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
                        (
                            name.clone(),
                            shape,
                            ty,
                            reads_self,
                            shadowed_origin,
                            prior_alias,
                            prior_shares,
                        )
                    })
                    .collect();
                let pre_st = st.clone();
                let mut overwritten = Vec::new();
                for (
                    name,
                    shape,
                    ty,
                    reads_self,
                    shadowed_origin,
                    prior_alias,
                    prior_shares,
                ) in resolved
                {
                    overwritten.push(name.clone());
                    let carries = ck.shape_may_share(&shape);
                    mark_nested_overwritten(st, &name, nested);
                    ck.define_binding(st, &name, shape.clone(), ty, immutable, managed, is_let);
                    if reads_self {
                        if let Some(pa) = prior_alias {
                            st.aliases.insert(name.clone(), pa);
                        }
                        for peer in prior_shares {
                            st.share_edge(&name, &peer);
                        }
                    }
                    if let Some(v) = val_opt {
                        ck.record_storage_from_snapshot(st, &pre_st, &name, v, &overwritten, true, true);
                    }
                    if let Some(origin) = shadowed_origin {
                        if let Some(b) = st.env_lookup_mut(&name) {
                            if b.non_owned_origin.is_none() {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                    }
                    if carries {
                        if let Some(origin) = &moved_non_owned {
                            if let Some(b) = st.env_lookup_mut(&name) {
                                if b.non_owned_origin.is_none() {
                                    b.non_owned_origin = Some(origin.clone());
                                }
                            }
                        }
                        if let Some(v) = val_opt {
                            ck.record_move_const_and_a3_loans(st, &name, v, 0);
                        }
                    }
                }
                if let Some(v) = l.values.first() {
                    ck.link_multi_value_return_aliases(st, v, &l.names);
                }
            }
            sync_nested_moves(st, nested);
        }
        Stmt::Task { callee, args, span } => {
            ck.mark_move_in_value(
                st,
                callee,
                format!("the spawn callee `{}`", callee.render()),
            );
            let desc = format!("the spawn `task {}(...)`", callee.render());
            let vals: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
            let targets = ck.spawn_param_shapes(st, callee, args.len());
            ck.check_site(
                st,
                *span,
                "HYB-E3001",
                &vals,
                managed,
                &desc,
                false,
                Some(callee),
                targets.as_deref(),
            );
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
        }
        Stmt::Send {
            chan,
            value,
            is_move,
            span,
        } => {
            ck.mark_move_in_value(
                st,
                chan,
                format!("the send target `{}`", chan.render()),
            );
            // Target classification (#77) first: a non-channel (or
            // unresolvable) send target is ill-typed — no value cascade.
            if ck.check_send_target(st, chan, *span) {
                let desc = if *is_move {
                    format!(
                        "the move-send `{} <- move {}`",
                        chan.render(),
                        value.render()
                    )
                } else {
                    format!("the send `{} <- {}`", chan.render(), value.render())
                };
                let vals: Vec<&Expr> = vec![value];
                let targets = ck.chan_elem_shape(st, chan);
                ck.check_site(st, *span, "HYB-E3002", &vals, managed, &desc, *is_move, None, targets.as_deref());
                ck.check_chan_send_const_overflow(st, chan, value);
            }
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                st.scope_push();
                match &arm.kind {
                    SelectArmKind::Send {
                        chan,
                        value,
                        is_move,
                    } => {
                        ck.mark_move_in_value(
                            st,
                            chan,
                            format!("the select-send target `{}`", chan.render()),
                        );
                        let span = chan.span().to(value.span());
                        if ck.check_send_target(st, chan, span) {
                            // C5: a select-send clause takes the SAME rules as
                            // a statement send — including the class-3
                            // owned-move route (`ch <- move v`). The parser's
                            // `is_move` marker used to be dropped when a send
                            // statement became a select arm.
                            let desc = if *is_move {
                                format!(
                                    "the move-send `{} <- move {}`",
                                    chan.render(),
                                    value.render()
                                )
                            } else {
                                format!("the select-send `{} <- {}`", chan.render(), value.render())
                            };
                            let vals: Vec<&Expr> = vec![value];
                            let targets = ck.chan_elem_shape(st, chan);
                            ck.check_site(st, span, "HYB-E3002", &vals, managed, &desc, *is_move, None, targets.as_deref());
                            ck.check_chan_send_const_overflow(st, chan, value);
                        }
                        sync_nested_moves(st, nested);
                        collect_expr_direct_fnlits(chan, st, nested);
                        collect_expr_direct_fnlits(value, st, nested);
                    }
                    SelectArmKind::Recv { names, chan } => {
                        ck.mark_move_in_value(
                            st,
                            chan,
                            format!("the select-recv `{}`", chan.render()),
                        );
                        sync_nested_moves(st, nested);
                        collect_expr_direct_fnlits(chan, st, nested);
                        // element shape when the channel resolves; `ok` is a
                        // bool; unresolvable stays Unknown (fail-closed, O4.9)
                        let elem = match ck.shape_of_expr(st, chan) {
                            TypeShape::Chan(e) => *e,
                            _ => TypeShape::Unknown,
                        };
                        for (i, n) in names.iter().enumerate() {
                            let shape = if i == 0 {
                                elem.clone()
                            } else {
                                TypeShape::Primitive
                            };
                            mark_nested_overwritten(st, n, nested);
                            ck.define_binding(st, n, shape, None, false, managed, false);
                        }
                        sync_nested_moves(st, nested);
                    }
                    SelectArmKind::Default => {}
                }
                check_body_boundaries(ck, st, &arm.body, managed, nested);
                pop_scope_and_sync(st, nested);
            }
        }
        Stmt::Managed {
            reason,
            category,
            tickets,
            body,
            span,
        } => {
            ck.audit_managed_block(reason, category, tickets, *span);
            st.scope_push();
            check_body_boundaries(ck, st, body, true, nested);
            pop_scope_and_sync(st, nested);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            // ONE implicit scope for the if (init vars visible through the
            // construct, Go rule); the then/else bodies open their own.
            st.scope_push();
            if let Some(i) = init {
                check_stmt_boundaries(ck, st, i, managed, nested);
            }
            check_stmt_own_uses(ck, st, stmt, managed);
            ck.mark_move_in_value(
                st,
                cond,
                format!("the condition `{}`", cond.render()),
            );
            sync_nested_moves(st, nested);
            collect_expr_direct_fnlits(cond, st, nested);
            st.scope_push();
            check_body_boundaries(ck, st, then, managed, nested);
            pop_scope_and_sync(st, nested);
            if let Some(o) = otherwise {
                check_stmt_boundaries(ck, st, o, managed, nested);
            }
            pop_scope_and_sync(st, nested);
        }
        Stmt::For { kind, body, .. } => {
            let outer_depth = st.scopes.len();
            let pre_moved: std::collections::HashSet<String> = st
                .env
                .iter()
                .chain(st.scopes.iter().flat_map(|s| s.iter()))
                .filter(|(_, b)| b.moved.is_some())
                .map(|(k, _)| k.clone())
                .collect();
            st.scope_push(); // the loop vars' scope
            match kind {
                Iterate { vars, expr } => {
                    ck.mark_move_in_value(
                        st,
                        expr,
                        format!("the loop header `{}`", expr.render()),
                    );
                    sync_nested_moves(st, nested);
                    collect_expr_direct_fnlits(expr, st, nested);
                    let is_chan = matches!(ck.shape_of_expr(st, expr), TypeShape::Chan(_));
                    let shapes = ck.iterate_var_shapes(st, expr, vars.len());
                    for (name, shape) in vars.iter().zip(shapes) {
                        let is_ref = shape.is_reference_alias();
                        let may_share = ck.shape_may_share(&shape);
                        mark_nested_overwritten(st, name, nested);
                        ck.define_binding(st, name, shape, None, false, managed, false);
                        if !is_chan {
                            if may_share {
                                ck.record_may_share(st, name, expr);
                            }
                            if is_ref {
                                if let Some(root) = expr.root_ident() {
                                    if ck.lookup_binding(st, root).is_some() {
                                        st.aliases.insert(name.clone(), root.to_string());
                                    }
                                }
                            }
                            if let Some(b) = st.env_lookup_mut(name) {
                                b.non_owned_origin = Some(format!(
                                    "range variable `{}` iterating `{}` (the collection still holds the element)",
                                    name,
                                    expr.render()
                                ));
                            }
                        }
                    }
                    sync_nested_moves(st, nested);
                }
                Range { var, lo, hi, .. } => {
                    ck.mark_move_in_value(
                        st,
                        lo,
                        format!("the range bound `{}`", lo.render()),
                    );
                    ck.mark_move_in_value(
                        st,
                        hi,
                        format!("the range bound `{}`", hi.render()),
                    );
                    sync_nested_moves(st, nested);
                    collect_expr_direct_fnlits(lo, st, nested);
                    collect_expr_direct_fnlits(hi, st, nested);
                    mark_nested_overwritten(st, var, nested);
                    ck.define_binding(st, var, TypeShape::Primitive, None, false, managed, false);
                    sync_nested_moves(st, nested);
                }
                While { cond } => {
                    ck.mark_move_in_value(
                        st,
                        cond,
                        format!("the loop condition `{}`", cond.render()),
                    );
                    sync_nested_moves(st, nested);
                    collect_expr_direct_fnlits(cond, st, nested);
                }
                Infinite => {}
            }
            st.scope_push(); // the body's own scope
            check_body_boundaries(ck, st, body, managed, nested);
            if !managed {
                let flat_body = crate::walk::flatten(body);
                let mut loop_stmts: Vec<(&Stmt, usize)> =
                    Vec::with_capacity(flat_body.stmts.len() + 1);
                loop_stmts.push((stmt, 0));
                for (s, idx) in flat_body.stmts {
                    loop_stmts.push((s, idx + 1));
                }
                let loop_events: std::collections::HashMap<usize, Vec<crate::walk::ScopeEvent>> =
                    flat_body
                        .events
                        .into_iter()
                        .map(|(k, v)| (k + 1, v))
                        .collect();
                let mut reported_backedge = std::collections::HashSet::new();
                ck.check_loop_backedge(
                    st,
                    outer_depth,
                    &loop_stmts,
                    &loop_events,
                    body,
                    &pre_moved,
                    &mut reported_backedge,
                );
            }
            pop_scope_and_sync(st, nested);
            pop_scope_and_sync(st, nested);
        }
        Stmt::Match {
            subject,
            arms,
            span,
        } => {
            ck.mark_move_in_value(
                st,
                subject,
                format!("the match subject `{}`", subject.render()),
            );
            sync_nested_moves(st, nested);
            collect_expr_direct_fnlits(subject, st, nested);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    ck.mark_move_in_value(
                        st,
                        g,
                        format!("the match guard `{}`", g.render()),
                    );
                }
            }
            ck.check_match(st, subject, arms, *span);
            for arm in arms {
                st.scope_push();
                if let hy_syntax::MatchPattern::Path {
                    binding: Some(name),
                    ..
                } = &arm.pattern
                {
                    ck.bind_match_arm(st, name, subject, managed);
                }
                if let Some(g) = &arm.guard {
                    ck.mark_move_in_value(
                        st,
                        g,
                        format!("the match guard `{}`", g.render()),
                    );
                    sync_nested_moves(st, nested);
                    collect_expr_direct_fnlits(g, st, nested);
                }
                check_body_boundaries(ck, st, &arm.body, managed, nested);
                pop_scope_and_sync(st, nested);
            }
        }
        Stmt::Block { block, .. } => {
            st.scope_push();
            check_body_boundaries(ck, st, block, managed, nested);
            pop_scope_and_sync(st, nested);
        }
        Stmt::Assign {
            targets,
            op,
            values,
            ..
        } => {
            for t in targets {
                ck.mark_move_in_place(st, t);
            }
            for v in values {
                ck.mark_move_in_value(st, v, format!("the move-assign `{}`", v.render()));
            }
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
            ck.process_closure_assign(st, targets, *op, values);
            sync_nested_moves(st, nested);
            if targets.len() == values.len() {
                for (t, v) in targets.iter().zip(values.iter()) {
                    if matches!(t.kind, hy_syntax::PlaceKind::Field { .. }) {
                        let is_borrow = crate::model::as_addr_of(v).is_some()
                            || matches!(ck.shape_of_expr(st, v), TypeShape::BorrowHandle(_));
                        if is_borrow {
                            ck.emit_a6_field_borrow_error(t);
                        }
                    }
                }
            } else if values.len() == 1 && targets.len() > 1 {
                let shapes = ck.multi_value_shapes(st, &values[0], targets.len());
                for (t, shape) in targets.iter().zip(shapes.iter()) {
                    if matches!(t.kind, hy_syntax::PlaceKind::Field { .. })
                        && matches!(shape, TypeShape::BorrowHandle(_))
                    {
                        ck.emit_a6_field_borrow_error(t);
                    }
                }
            }
            for (t, v) in targets.iter().zip(values.iter()) {
                if Checker::is_nil_literal(v) {
                    if let Some(shape) = ck.place_shape(st, t) {
                        if !matches!(shape, TypeShape::Unknown)
                            && !Checker::is_nilable_shape(&shape)
                        {
                            ck.emit_nil_type_error(
                                &shape,
                                v.span(),
                                &format!("the assignment target `{}`", t.render()),
                            );
                        }
                    }
                }
                ck.check_composite_nils(st, v);
            }
        }
        Stmt::Return { values, span } => {
            ck.process_return(st, values, *span);
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
        }
        Stmt::Expr { expr, .. } => {
            ck.mark_move_in_value(
                st,
                expr,
                format!("the expression `{}`", expr.render()),
            );
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
        }
        Stmt::Defer { call, .. } => {
            ck.mark_move_in_value(
                st,
                call,
                format!("the deferred call `defer {}`", call.render()),
            );
            sync_nested_moves(st, nested);
            collect_stmt_direct_fnlits(stmt, st, nested);
        }
        _ => {}
    }
}

/// Check A3 (`HYB-E3102`) for map-slot loans opened inside a closure body.
fn check_closure_slot_loans(ck: &mut Checker<'_>, st: &mut FnState, body: &Block) {
    use crate::model::{Loan, LoanTarget};
    use crate::walk;
    let flat = walk::flatten(body);
    let saved_last_use = std::mem::replace(&mut st.last_use, flat.last_use.clone());
    let saved_loop_end = std::mem::replace(&mut st.loop_end, flat.loop_end.clone());
    let saved_loans = std::mem::take(&mut st.loans);
    for (stmt, idx) in &flat.stmts {
        let idx = *idx;
        let mut calls = Vec::new();
        walk::own_calls(stmt, &mut calls);
        for cr in calls {
            let mut dummy_consumes = Vec::new();
            ck.process_call(st, &cr, idx, None, &mut dummy_consumes);
        }
        match stmt {
            Stmt::Local(l) => {
                if l.names.len() == l.values.len() {
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        if let Some(path) = walk::addr_of_index_base_path(val) {
                            let binding = name.clone();
                            let key = walk::addr_of_index_slot_key(val);
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
                            ck.extend_map_slot_loan_to_alias(st, name, val, idx);
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let shapes = ck.multi_value_shapes(st, &l.values[0], l.names.len());
                    for (name, shape) in l.names.iter().zip(shapes.iter()) {
                        if shape.is_reference_alias() {
                            ck.extend_map_slot_loan_to_alias(st, name, &l.values[0], idx);
                        }
                    }
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                ..
            } => {
                if *op == hy_syntax::AssignOp::Assign && targets.len() == values.len() {
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let hy_syntax::PlaceKind::Ident(name) = &t.kind {
                            if let Some(path) = walk::addr_of_index_base_path(v) {
                                let binding = name.clone();
                                let key = walk::addr_of_index_slot_key(v);
                                let last =
                                    st.last_use.get(&binding).copied().unwrap_or(idx).max(idx);
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
                                ck.extend_map_slot_loan_to_alias(st, name, v, idx);
                            }
                        }
                    }
                } else if *op == hy_syntax::AssignOp::Assign
                    && values.len() == 1
                    && targets.len() > 1
                {
                    let shapes = ck.multi_value_shapes(st, &values[0], targets.len());
                    for (t, shape) in targets.iter().zip(shapes.iter()) {
                        if let hy_syntax::PlaceKind::Ident(name) = &t.kind {
                            if shape.is_reference_alias() {
                                ck.extend_map_slot_loan_to_alias(st, name, &values[0], idx);
                            }
                        }
                    }
                }
                for t in targets {
                    if matches!(t.kind, hy_syntax::PlaceKind::Index { .. }) {
                        if let Some(path) = walk::place_path(t) {
                            ck.emit_a3_if_live_pub(st, &path, t.span, idx, "insert");
                        }
                    }
                }
            }
            _ => {}
        }
    }
    st.loans = saved_loans;
    st.loop_end = saved_loop_end;
    st.last_use = saved_last_use;
}

/// The outermost FnLits reachable from `block`. Closures nested inside other
/// closures are NOT collected here: check_one_closure finds them while walking
/// the enclosing closure's body (collect_stmt_direct_fnlits) and checks them
/// recursively against the enclosing closure's environment (#229).
/// walk.rs's use/call collectors skip FnLit deliberately (A5).
fn collect_fnlits(
    block: &Block,
) -> Vec<(
    &[hy_syntax::Param],
    &[hy_syntax::FnResult],
    &Block,
    (usize, usize),
)> {
    let mut out = Vec::new();
    for s in &block.stmts {
        collect_stmt(s, &mut out);
    }
    out
}

fn collect_stmt<'a>(
    stmt: &'a Stmt,
    out: &mut Vec<(
        &'a [hy_syntax::Param],
        &'a [hy_syntax::FnResult],
        &'a Block,
        (usize, usize),
    )>,
) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                collect_expr(v, out);
            }
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                collect_place(t, out);
            }
            for v in values {
                collect_expr(v, out);
            }
        }
        Stmt::Expr { expr, .. } => collect_expr(expr, out),
        Stmt::Send { chan, value, .. } => {
            collect_expr(chan, out);
            collect_expr(value, out);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                collect_stmt(i, out);
            }
            collect_expr(cond, out);
            collect_block(then, out);
            if let Some(o) = otherwise {
                collect_stmt(o, out);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                While { cond } => collect_expr(cond, out),
                Range { lo, hi, .. } => {
                    collect_expr(lo, out);
                    collect_expr(hi, out);
                }
                Iterate { expr, .. } => collect_expr(expr, out),
                Infinite => {}
            }
            collect_block(body, out);
        }
        Stmt::Return { values, .. } => {
            for v in values {
                collect_expr(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            collect_expr(callee, out);
            for a in args {
                collect_expr(&a.value, out);
            }
        }
        Stmt::Managed { body, .. } => collect_block(body, out),
        Stmt::Match { subject, arms, .. } => {
            collect_expr(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    collect_expr(g, out);
                }
                collect_block(&arm.body, out);
            }
        }
        Stmt::Defer { call, .. } => collect_expr(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Block { block, .. } => collect_block(block, out),
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        collect_expr(chan, out);
                        collect_expr(value, out);
                    }
                    SelectArmKind::Recv { chan, .. } => collect_expr(chan, out),
                    SelectArmKind::Default => {}
                }
                collect_block(&arm.body, out);
            }
        }
    }
}

fn collect_place<'a>(
    place: &'a hy_syntax::Place,
    out: &mut Vec<(
        &'a [hy_syntax::Param],
        &'a [hy_syntax::FnResult],
        &'a Block,
        (usize, usize),
    )>,
) {
    match &place.kind {
        hy_syntax::PlaceKind::Ident(_) => {}
        hy_syntax::PlaceKind::Index { base, index } => {
            collect_place(base, out);
            collect_expr(index, out);
        }
        hy_syntax::PlaceKind::Field { base, .. } => collect_place(base, out),
        hy_syntax::PlaceKind::AddrOf { base } => collect_place(base, out),
    }
}

fn collect_block<'a>(
    block: &'a Block,
    out: &mut Vec<(
        &'a [hy_syntax::Param],
        &'a [hy_syntax::FnResult],
        &'a Block,
        (usize, usize),
    )>,
) {
    for s in &block.stmts {
        collect_stmt(s, out);
    }
}

fn collect_expr<'a>(
    expr: &'a Expr,
    out: &mut Vec<(
        &'a [hy_syntax::Param],
        &'a [hy_syntax::FnResult],
        &'a Block,
        (usize, usize),
    )>,
) {
    match expr {
        Expr::Ident { .. } => {}
        Expr::FnLit {
            params,
            results,
            body,
            span,
        } => {
            // Only collect outermost closures here; nested closures are
            // checked recursively inside check_one_closure against their
            // enclosing closure's environment (#229).
            out.push((
                params,
                results,
                body,
                (span.start.line as usize, span.start.col as usize),
            ));
        }
        Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let hy_syntax::InterpPart::Expr(e) = p {
                    collect_expr(e, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            collect_expr(lhs, out);
            collect_expr(rhs, out);
        }
        Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } => collect_expr(operand, out),
        Expr::Move { operand, .. } | Expr::Try { operand, .. } => collect_expr(operand, out),
        Expr::Receive { chan, .. } => collect_expr(chan, out),
        Expr::Index { base, index, .. } => {
            collect_expr(base, out);
            collect_expr(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            collect_expr(base, out);
            if let Some(lo) = lo {
                collect_expr(lo, out);
            }
            if let Some(hi) = hi {
                collect_expr(hi, out);
            }
        }
        Expr::Field { base, .. } => collect_expr(base, out),
        Expr::Call { callee, args, .. } => {
            collect_expr(callee, out);
            for a in args {
                collect_expr(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                collect_expr(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                collect_expr(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                collect_expr(k, out);
                collect_expr(v, out);
            }
        }
        Expr::Paren { inner, .. } => collect_expr(inner, out),
    }
}

// ---------------------------------------------------------------------------
// Deferred-call use-after-move checks (issues #415, #438)
// ---------------------------------------------------------------------------

enum LaterDeferFate {
    Moved(crate::model::MoveInfo),
    ReboundBeforeMove,
    None,
}

/// Check every `defer` statement in `body`: because a deferred call executes
/// at function/closure exit, any binding captured by `defer` remains live
/// until exit. If that binding (or any pre-/post-defer alias or may-share peer
/// still referencing that storage) is moved or consumed after the `defer`
/// statement, the deferred call races with the new owner or reads a consumed
/// handle (HYB-E3003, O5.1/O5.2/O13).
fn check_deferred_uses(ck: &mut Checker<'_>, st: &FnState, body: &Block) {
    let flat = crate::walk::flatten(body);
    for (stmt, defer_idx) in &flat.stmts {
        let Stmt::Defer { .. } = stmt else {
            continue;
        };
        if flat.in_managed(*defer_idx) {
            continue;
        }
        let mut uses = Vec::new();
        crate::walk::own_uses(stmt, &mut uses);

        for u in uses {
            if u.via_move {
                continue;
            }
            if ck
                .diags
                .iter()
                .any(|d| d.code == "HYB-E3003" && d.span == u.span)
            {
                continue;
            }
            let fate = inspect_later_defer_fate(ck, st, &flat, *defer_idx, &u.name);
            if matches!(fate, LaterDeferFate::ReboundBeforeMove) {
                continue;
            }
            let before_len = ck.diags.len();
            ck.check_uses(st, std::slice::from_ref(&u));
            if ck.diags.len() == before_len {
                if let LaterDeferFate::Moved(mv) = fate {
                    let mut tmp_st = FnState::default();
                    let shape = ck
                        .lookup_binding(st, &u.name)
                        .map(|b| b.shape.clone())
                        .unwrap_or(TypeShape::Map(
                            Box::new(TypeShape::Str),
                            Box::new(TypeShape::Primitive),
                        ));
                    tmp_st.env.insert(
                        u.name.clone(),
                        Binding {
                            shape,
                            ty: None,
                            immutable: false,
                            is_let: false,
                            managed_taint: false,
                            moved: Some(mv),
                            non_owned_origin: None,
                            slot_loan: false,
                        },
                    );
                    ck.check_uses(&tmp_st, std::slice::from_ref(&u));
                }
            }
        }
    }
}

fn inspect_later_defer_fate(
    ck: &Checker<'_>,
    st: &FnState,
    flat: &crate::walk::FlatInfo<'_>,
    defer_idx: usize,
    name: &str,
) -> LaterDeferFate {
    let mut pre_peers: std::collections::HashMap<String, std::collections::HashSet<String>> =
        std::collections::HashMap::new();
    for (stmt, idx) in &flat.stmts {
        if *idx >= defer_idx {
            break;
        }
        update_pre_defer_peers(ck, st, stmt, &mut pre_peers);
    }

    let root = crate::freeze::resolve_alias(st, name);
    let mut watch: std::collections::HashSet<String> = st.share_group(name);
    watch.insert(name.to_string());
    watch.insert(root.clone());
    for a in st.aliases.keys() {
        if crate::freeze::resolve_alias(st, a) == root {
            watch.insert(a.clone());
        }
    }
    let mut stack: Vec<String> = watch.iter().cloned().collect();
    while let Some(cur) = stack.pop() {
        if let Some(peers) = pre_peers.get(&cur) {
            for p in peers {
                if watch.insert(p.clone()) {
                    stack.push(p.clone());
                }
            }
        }
    }

    let mut rebound_self = false;
    for (stmt, idx) in &flat.stmts {
        if *idx <= defer_idx || flat.in_managed(*idx) {
            continue;
        }
        if let Some(mv) = stmt_moves_watched_binding(stmt, &watch) {
            return LaterDeferFate::Moved(mv);
        }
        update_post_defer_watch(ck, st, stmt, name, &mut watch, &mut rebound_self);
    }
    if rebound_self {
        LaterDeferFate::ReboundBeforeMove
    } else {
        LaterDeferFate::None
    }
}

fn sever_peer_node(
    peers: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
    target: &str,
) {
    if let Some(linked) = peers.remove(target) {
        let list: Vec<String> = linked.into_iter().collect();
        for p in &list {
            if let Some(set) = peers.get_mut(p) {
                set.remove(target);
            }
        }
        for i in 0..list.len() {
            for j in (i + 1)..list.len() {
                peers
                    .entry(list[i].clone())
                    .or_default()
                    .insert(list[j].clone());
                peers
                    .entry(list[j].clone())
                    .or_default()
                    .insert(list[i].clone());
            }
        }
    }
}

fn link_peer_nodes(
    peers: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
    a: &str,
    b: &str,
) {
    if a == b || a == "_" || b == "_" {
        return;
    }
    peers.entry(a.to_string()).or_default().insert(b.to_string());
    peers.entry(b.to_string()).or_default().insert(a.to_string());
}

fn expr_referenced_bindings(e: &Expr, out: &mut Vec<String>) {
    let mut uses = Vec::new();
    crate::walk::expr_uses(e, &mut uses);
    for u in uses {
        if !u.via_move {
            out.push(u.name);
        }
    }
}

fn update_pre_defer_peers(
    ck: &Checker<'_>,
    st: &FnState,
    stmt: &Stmt,
    peers: &mut std::collections::HashMap<String, std::collections::HashSet<String>>,
) {
    match stmt {
        Stmt::Local(l) => {
            for (i, target) in l.names.iter().enumerate() {
                if target == "_" {
                    continue;
                }
                let val = l.values.get(i).or_else(|| l.values.first());
                let mut refs = Vec::new();
                if let Some(v) = val {
                    expr_referenced_bindings(v, &mut refs);
                }
                let self_read = val.is_some_and(|v| ck.value_reads_storage_of(st, v, target));
                if !self_read {
                    sever_peer_node(peers, target);
                }
                for r in refs {
                    let may_share = ck
                        .lookup_binding(st, &r)
                        .map(|b| ck.shape_may_share(&b.shape))
                        .unwrap_or(true);
                    if may_share {
                        link_peer_nodes(peers, target, &r);
                    }
                }
            }
            if l.values.len() == 1 && l.names.len() > 1 {
                for (i, j) in ck.callee_shared_return_pairs(st, &l.values[0]) {
                    if let (Some(a), Some(b)) = (l.names.get(i), l.names.get(j)) {
                        link_peer_nodes(peers, a, b);
                    }
                }
            }
        }
        Stmt::Assign {
            targets,
            op: hy_syntax::AssignOp::Assign,
            values,
            ..
        } => {
            for (i, t) in targets.iter().enumerate() {
                if let hy_syntax::PlaceKind::Ident(target) = &t.kind {
                    if target == "_" {
                        continue;
                    }
                    let val = values.get(i).or_else(|| values.first());
                    let mut refs = Vec::new();
                    if let Some(v) = val {
                        expr_referenced_bindings(v, &mut refs);
                    }
                    let self_read = val.is_some_and(|v| ck.value_reads_storage_of(st, v, target));
                    if !self_read {
                        sever_peer_node(peers, target);
                    }
                    for r in refs {
                        let may_share = ck
                            .lookup_binding(st, &r)
                            .map(|b| ck.shape_may_share(&b.shape))
                            .unwrap_or(true);
                        if may_share {
                            link_peer_nodes(peers, target, &r);
                        }
                    }
                }
            }
            if values.len() == 1 && targets.len() > 1 {
                for (i, j) in ck.callee_shared_return_pairs(st, &values[0]) {
                    if let (Some(ta), Some(tb)) = (targets.get(i), targets.get(j)) {
                        if let (
                            hy_syntax::PlaceKind::Ident(a),
                            hy_syntax::PlaceKind::Ident(b),
                        ) = (&ta.kind, &tb.kind)
                        {
                            link_peer_nodes(peers, a, b);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn update_post_defer_watch(
    ck: &Checker<'_>,
    st: &FnState,
    stmt: &Stmt,
    deferred_name: &str,
    watch: &mut std::collections::HashSet<String>,
    rebound_self: &mut bool,
) {
    let mut handle_bind = |target: &str, val: Option<&Expr>| {
        if target == "_" {
            return;
        }
        let Some(v) = val else {
            return;
        };
        let reads_watched = watch
            .iter()
            .any(|w| ck.value_reads_storage_of(st, v, w));
        if reads_watched {
            watch.insert(target.to_string());
        } else {
            watch.remove(target);
            if target == deferred_name {
                *rebound_self = true;
            }
        }
    };

    match stmt {
        Stmt::Local(l) => {
            for (i, target) in l.names.iter().enumerate() {
                let val = l.values.get(i).or_else(|| l.values.first());
                handle_bind(target, val);
            }
        }
        Stmt::Assign {
            targets,
            op: hy_syntax::AssignOp::Assign,
            values,
            ..
        } => {
            for (i, t) in targets.iter().enumerate() {
                if let hy_syntax::PlaceKind::Ident(target) = &t.kind {
                    let val = values.get(i).or_else(|| values.first());
                    handle_bind(target, val);
                }
            }
        }
        _ => {}
    }
}

fn stmt_moves_watched_binding(
    stmt: &Stmt,
    watch: &std::collections::HashSet<String>,
) -> Option<crate::model::MoveInfo> {
    let check_expr = |e: &Expr, desc: String| -> Option<crate::model::MoveInfo> {
        let mut ops = Vec::new();
        crate::model::collect_move_operands(e, &mut ops);
        for (op, mspan) in ops {
            let moved_name = match op {
                Expr::Ident { name, .. } => Some(name.as_str()),
                Expr::Field { .. } => op.root_ident(),
                _ => None,
            };
            if let Some(n) = moved_name {
                if watch.contains(n) {
                    return Some(crate::model::MoveInfo {
                        span: mspan,
                        kind: crate::model::MoveKind::Moved,
                        desc,
                        moved_name: n.to_string(),
                    });
                }
            }
        }
        None
    };

    match stmt {
        Stmt::Task { args, .. } => {
            for a in args {
                let desc = format!("the spawn argument `{}`", a.value.render());
                if let Some(mv) = check_expr(&a.value, desc) {
                    return Some(mv);
                }
            }
        }
        Stmt::Send {
            chan,
            value,
            is_move,
            span,
        } => {
            let desc = format!(
                "the move-send `{} <- {}`",
                chan.render(),
                if *is_move && !matches!(value, Expr::Move { .. }) {
                    format!("move {}", value.render())
                } else {
                    value.render()
                }
            );
            if *is_move {
                if let Expr::Ident { name, .. } = crate::model::unwrap_parens(value) {
                    if watch.contains(name) {
                        return Some(crate::model::MoveInfo {
                            span: *span,
                            kind: crate::model::MoveKind::Moved,
                            desc,
                            moved_name: name.to_string(),
                        });
                    }
                }
            }
            if let Some(mv) = check_expr(value, desc) {
                return Some(mv);
            }
        }
        Stmt::Local(l) => {
            for v in &l.values {
                let desc = format!("the move-binding `{}`", v.render());
                if let Some(mv) = check_expr(v, desc) {
                    return Some(mv);
                }
            }
        }
        Stmt::Assign { values, .. } => {
            for v in values {
                let desc = format!("the move-assignment `{}`", v.render());
                if let Some(mv) = check_expr(v, desc) {
                    return Some(mv);
                }
            }
        }
        Stmt::Return { values, .. } => {
            for v in values {
                let desc = format!("the move-return `{}`", v.render());
                if let Some(mv) = check_expr(v, desc) {
                    return Some(mv);
                }
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                if let SelectArmKind::Send { chan, value, .. } = &arm.kind {
                    let desc = format!(
                        "the select move-send `{} <- {}`",
                        chan.render(),
                        value.render()
                    );
                    if let Some(mv) = check_expr(value, desc) {
                        return Some(mv);
                    }
                }
            }
        }
        _ => {}
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::check_file;
    use hy_syntax::parse_file;

    fn codes(src: &str) -> Vec<String> {
        let file = parse_file("test.hy", src).expect("parse");
        let checked = check_file(&file);
        checked.diagnostics.into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn reject_defer_call_before_task_move_of_same_binding() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    shared := map[string]int{"k": 1}
    defer report(shared)
    task worker(move shared)
}
"#;
        assert_eq!(codes(src), vec!["HYB-E3003"]);
    }

    #[test]
    fn reject_defer_call_on_alias_before_move_of_origin() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    shared := map[string]int{"k": 1}
    alias := shared
    defer report(alias)
    task worker(move shared)
}
"#;
        assert_eq!(codes(src), vec!["HYB-E3003"]);
    }

    #[test]
    fn reject_defer_in_nested_block_and_closure_before_move() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    if true {
        shared := map[string]int{"k": 1}
        defer report(shared)
        task worker(move shared)
    }
    f := fn() {
        inner := map[string]int{"k": 1}
        defer report(inner)
        task worker(move inner)
    }
    f()
}
"#;
        assert_eq!(codes(src), vec!["HYB-E3003", "HYB-E3003"]);
    }

    #[test]
    fn reject_deferred_closure_literal_capturing_binding_moved_later() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn main() {
    shared := map[string]int{"k": 1}
    defer (fn() {
        fmt.println(shared["k"])
    })()
    task worker(move shared)
}
"#;
        assert_eq!(codes(src), vec!["HYB-E3003"]);
    }

    #[test]
    fn accept_defer_when_unmoved_or_rebound_before_move() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    keep := map[string]int{"a": 1}
    defer report(keep)
    shared := map[string]int{"k": 1}
    defer report(shared)
    shared = map[string]int{"k": 2}
    task worker(move shared)
}
"#;
        assert!(codes(src).is_empty());
    }

    #[test]
    fn reject_rebind_of_deferred_name_when_pre_defer_alias_moves_issue_438() {
        let src = r#"import "fmt"

fn worker(owned map[string]int) {
    owned["k"] = 2
}

fn report(m map[string]int) {
    fmt.println(m["k"])
}

fn main() {
    shared := map[string]int{"k": 1}
    alias := shared
    defer report(shared)
    shared = map[string]int{}
    task worker(move alias)
}
"#;
        assert_eq!(codes(src), vec!["HYB-E3003"]);
    }
}
