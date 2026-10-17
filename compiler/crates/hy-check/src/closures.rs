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
    for (params, results, body) in collect_fnlits(body) {
        check_one_closure(ck, st, params, results, body);
    }
}

type NestedClosure<'a> = (
    &'a [hy_syntax::Param],
    &'a [hy_syntax::FnResult],
    &'a Block,
    Vec<std::collections::HashMap<String, Binding>>,
    std::collections::HashMap<String, String>,
    std::collections::HashMap<String, std::collections::HashSet<String>>,
);

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
    check_body_boundaries(ck, &mut cst, body, false, &mut nested);
    for (nparams, nresults, nbody, mut saved_scopes, saved_aliases, saved_share) in nested {
        if !saved_scopes.is_empty() && !cst.scopes.is_empty() {
            for (k, b) in &cst.scopes[0] {
                match saved_scopes[0].get_mut(k) {
                    Some(sb) => {
                        if sb.moved.is_none() && b.moved.is_some() {
                            sb.moved = b.moved.clone();
                        }
                    }
                    None => {
                        saved_scopes[0].insert(k.clone(), b.clone());
                    }
                }
            }
        }
        let mut snap = cst.clone();
        snap.scopes = saved_scopes;
        // A nested closure checks against ITS OWN named results: reset the
        // enclosing closure's seeded state, check_one_closure re-seeds from
        // nresults (a bare return inside the nested closure must not record
        // uses for the enclosing closure's result names).
        snap.named_results = Vec::new();
        snap.result_shapes = Vec::new();
        for (k, v) in saved_aliases {
            snap.aliases.entry(k).or_insert(v);
        }
        for (k, vs) in saved_share {
            snap.may_share.entry(k).or_default().extend(vs);
        }
        check_one_closure(ck, &snap, nparams, nresults, nbody);
    }
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
    for stmt in &block.stmts {
        collect_stmt_direct_fnlits(stmt, st, nested);
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
        if let Expr::FnLit { params, results, body, .. } = crate::model::unwrap_parens(flit) {
            nested.push((
                params,
                results,
                body,
                st.scopes.clone(),
                st.aliases.clone(),
                st.may_share.clone(),
            ));
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

    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                ck.mark_move_in_value(st, v, format!("the move-define `{}`", v.render()));
            }
            // T22 (issue #12 parity with the flat pass): only `const` is
            // DEEPLY immutable; `let` is a SHALLOW immutable binding, so it
            // must NOT take the class-4 sendability shortcut.
            let immutable = l.kind == LocalKind::Const;
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
                    ck.define_binding(st, name, shape.clone(), ty.clone(), immutable, managed);
                }
            } else if l.names.len() == l.values.len() {
                for (name, value) in l.names.iter().zip(l.values.iter()) {
                    let moved_non_owned = crate::model::move_operand_ident(value)
                        .and_then(|src| ck.lookup_binding(st, &src))
                        .and_then(|b| b.non_owned_origin.clone())
                        .or_else(|| ck.moved_non_owned_origin_in_expr(st, value));
                    // ascription fills gaps (O4.9); inference keeps priority
                    // when it resolves (boundary handles stay precise)
                    let inferred = ck.shape_of_expr(st, value);
                    let (shape, ty) = match (&l.ty, inferred) {
                        (Some(t), TypeShape::Unknown) => (ck.shape_of_type(t), Some(t.clone())),
                        (_, inferred) => (
                            inferred,
                            ck.ty_of_expr(st, value).or_else(|| l.ty.clone()),
                        ),
                    };
                    if crate::model::move_operand_ident(value).is_some()
                        || matches!(crate::model::unwrap_parens(value), Expr::Move { .. })
                    {
                        ck.define_binding(st, name, shape, ty, immutable, managed);
                        if let Some(origin) = moved_non_owned {
                            if let Some(b) = st.env_lookup_mut(name) {
                                b.non_owned_origin = Some(origin);
                            }
                        }
                        continue;
                    }
                    let reads_self = ck.value_reads_storage_of(st, value, name);
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
                    ck.define_binding(st, name, shape, ty, immutable, managed);
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
                    // issue #137: track may_share edges and aliases inside closures
                    // so local aliases of non-owned captured bindings inherit origin
                    // and cannot bypass O5.1 / O5.4 move restrictions.
                    ck.record_may_share(st, name, value);
                    ck.record_alias(st, name, value);
                    if let Some(origin) = shadowed_origin {
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
                for ((name, shape), ty) in l.names.iter().zip(shapes).zip(tys) {
                    ck.define_binding(st, name, shape, ty, immutable, managed);
                }
            }
        }
        Stmt::Task { callee, args, span } => {
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
        }
        Stmt::Send {
            chan,
            value,
            is_move,
            span,
        } => {
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
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                st.scope_push();
                match &arm.kind {
                    SelectArmKind::Send { chan, value } => {
                        let span = chan.span().to(value.span());
                        if ck.check_send_target(st, chan, span) {
                            let desc =
                                format!("the select-send `{} <- {}`", chan.render(), value.render());
                            let vals: Vec<&Expr> = vec![value];
                            let targets = ck.chan_elem_shape(st, chan);
                            ck.check_site(st, span, "HYB-E3002", &vals, managed, &desc, false, None, targets.as_deref());
                        }
                    }
                    SelectArmKind::Recv { names, chan } => {
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
                            ck.define_binding(st, n, shape, None, false, managed);
                        }
                    }
                    SelectArmKind::Default => {}
                }
                check_body_boundaries(ck, st, &arm.body, managed, nested);
                st.scope_pop();
            }
        }
        Stmt::Managed { body, .. } => {
            ck.managed_blocks += 1; // O6.3 parity with the flat walk
            st.scope_push();
            check_body_boundaries(ck, st, body, true, nested);
            st.scope_pop();
        }
        Stmt::If {
            init,
            then,
            otherwise,
            ..
        } => {
            // ONE implicit scope for the if (init vars visible through the
            // construct, Go rule); the then/else bodies open their own.
            st.scope_push();
            if let Some(i) = init {
                check_stmt_boundaries(ck, st, i, managed, nested);
                collect_stmt_direct_fnlits(i, st, nested);
            }
            check_stmt_own_uses(ck, st, stmt, managed);
            st.scope_push();
            check_body_boundaries(ck, st, then, managed, nested);
            st.scope_pop();
            if let Some(o) = otherwise {
                check_stmt_boundaries(ck, st, o, managed, nested);
                // An else-if chain link is a statement FIELD, not a block
                // stmt, so no body's collection loop ever reaches it: collect
                // its own direct closures here (own_fnlits(If) yields the
                // cond only — its init is collected inside the recursive
                // walk above, its bodies by check_body_boundaries). A plain
                // `else { }` block is still collected by its body loop.
                if !matches!(o.as_ref(), Stmt::Block { .. }) {
                    collect_stmt_direct_fnlits(o, st, nested);
                }
            }
            st.scope_pop();
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
                    let is_chan = matches!(ck.shape_of_expr(st, expr), TypeShape::Chan(_));
                    let shapes = ck.iterate_var_shapes(st, expr, vars.len());
                    for (name, shape) in vars.iter().zip(shapes) {
                        let is_ref = shape.is_reference_alias();
                        ck.define_binding(st, name, shape, None, false, managed);
                        if !is_chan {
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
                }
                Range { var, .. } => {
                    ck.define_binding(st, var, TypeShape::Primitive, None, false, managed);
                }
                While { .. } | Infinite => {}
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
                let mut reported_backedge = std::collections::HashSet::new();
                ck.check_loop_backedge(
                    st,
                    outer_depth,
                    &loop_stmts,
                    body,
                    &pre_moved,
                    &mut reported_backedge,
                );
            }
            st.scope_pop();
            st.scope_pop();
        }
        Stmt::Match {
            subject,
            arms,
            span,
        } => {
            ck.check_match(st, subject, arms, *span);
            for arm in arms {
                st.scope_push();
                check_body_boundaries(ck, st, &arm.body, managed, nested);
                st.scope_pop();
            }
        }
        Stmt::Block { block, .. } => {
            st.scope_push();
            check_body_boundaries(ck, st, block, managed, nested);
            st.scope_pop();
        }
        Stmt::Assign {
            targets,
            op,
            values,
            ..
        } => {
            ck.process_closure_assign(st, targets, *op, values);
        }
        Stmt::Return { values, span } => {
            for v in values {
                ck.mark_move_in_value(st, v, format!("the move-return `{}`", v.render()));
            }
            ck.process_return(st, values, *span);
        }
        _ => {}
    }
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
                    SelectArmKind::Send { chan, value } => {
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
    )>,
) {
    match expr {
        Expr::Ident { .. } => {}
        Expr::FnLit { params, results, body, .. } => {
            // Only collect outermost closures here; nested closures are
            // checked recursively inside check_one_closure against their
            // enclosing closure's environment (#229).
            out.push((params, results, body));
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
