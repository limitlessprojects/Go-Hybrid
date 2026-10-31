//! freeze.rs — rules A1b / A7 (O5 freeze-after-send, O13 consume).
//!
//! After `move x` at a spawn/send/assignment/return — or after a consuming
//! API call — the binding AND every live alias are frozen: any use, read OR
//! write (D1 is RESOLVED: reads are banned; SPEC O5.2; corpus 19 pins the
//! read form, 20 the alias-write form), is HYB-E3003 with tier0_rule A1b
//! (moves) or A7 (consuming APIs, via `.with_rule("A7")`).

use crate::model::{Checker, FnState, MoveInfo, MoveKind};
use crate::walk::Use;
use hy_diag::Diagnostic;
use hy_syntax::Expr;
use std::collections::{HashMap, HashSet};

/// Follow an alias chain to its root. If a mutual rebind (`m2 := m; m = m2`)
/// created a cycle in `st.aliases`, return the lexicographically smallest
/// member of the cycle so every member of the cycle resolves to the same
/// canonical root.
pub(crate) fn resolve_alias(st: &FnState, name: &str) -> String {
    let mut cur = name.to_string();
    let mut seen: Vec<String> = vec![cur.clone()];
    for _ in 0..16 {
        match st.aliases.get(&cur) {
            Some(next) => {
                if let Some(pos) = seen.iter().position(|s| s == next) {
                    return seen[pos..]
                        .iter()
                        .min()
                        .cloned()
                        .unwrap_or_else(|| cur.clone());
                }
                cur = next.clone();
                seen.push(cur.clone());
            }
            None => break,
        }
    }
    cur
}

/// Transitive connected component of `seed` across both `aliases` (parent and
/// child directions) and `may_share`, including `seed` itself.
fn storage_component(st: &FnState, seed: &str) -> HashSet<String> {
    if !st.aliases.contains_key(seed)
        && st.may_share.get(seed).is_none_or(|peers| peers.is_empty())
        && !st.aliases.values().any(|parent| parent == seed)
    {
        return HashSet::from([seed.to_string()]);
    }
    let mut children: HashMap<&str, Vec<&String>> = HashMap::new();
    for (child, parent) in &st.aliases {
        children.entry(parent.as_str()).or_default().push(child);
    }
    let mut seen: HashSet<String> = HashSet::new();
    let mut stack = vec![seed.to_string()];
    while let Some(cur) = stack.pop() {
        if !seen.insert(cur.clone()) {
            continue;
        }
        if let Some(parent) = st.aliases.get(&cur) {
            if !seen.contains(parent) {
                stack.push(parent.clone());
            }
        }
        if let Some(children) = children.get(cur.as_str()) {
            for &child in children {
                if !seen.contains(child) {
                    stack.push(child.clone());
                }
            }
        }
        if let Some(peers) = st.may_share.get(&cur) {
            for p in peers {
                if !seen.contains(p) {
                    stack.push(p.clone());
                }
            }
        }
    }
    seen
}

fn same_or_shared(st: &FnState, a: &str, root_a: &str, b: &str, root_b: &str) -> bool {
    if root_a == root_b {
        return true;
    }
    let comp = storage_component(st, a);
    comp.contains(b) || comp.contains(root_b)
}

#[derive(Clone, Copy)]
struct LoopFlow {
    continue_reach: bool,
    loop_break_reach: bool,
    select_break_reach: Option<bool>,
}

impl<'a> Checker<'a> {
    // -----------------------------------------------------------------------
    // Use-after-move / use-after-consume detection
    // -----------------------------------------------------------------------

    /// Check one statement's uses against the frozen state, including
    /// same-statement double-moves (`task both(move m, move m)`) and
    /// same-statement move-plus-read (`a, b := move m, m`).
    pub(crate) fn check_uses(&mut self, st: &FnState, uses: &[Use]) {
        let mut reported: HashSet<String> = HashSet::new();
        let mut stmt_moves: Vec<(String, String, MoveInfo)> = Vec::new();
        for u in uses {
            let moved = match self.lookup_binding(st, &u.name) {
                Some(b) => b.moved.clone(),
                None => continue,
            };
            if let Some(mv) = moved {
                if reported.insert(u.name.clone()) {
                    self.emit_use_after_move(st, u, &mv);
                }
                continue;
            }
            let root = resolve_alias(st, &u.name);
            if let Some((_, _, prior_mv)) = stmt_moves
                .iter()
                .find(|(mv_name, r, _)| same_or_shared(st, &u.name, &root, mv_name, r))
            {
                if reported.insert(format!("{}@{}", u.name, u.span.start.line)) {
                    self.emit_use_after_move(st, u, prior_mv);
                }
                continue;
            }
            if u.via_move {
                stmt_moves.push((
                    u.name.clone(),
                    root,
                    MoveInfo {
                        span: u.span,
                        kind: MoveKind::Moved,
                        desc: format!("`move {}` earlier in the same statement", u.name),
                        moved_name: u.name.clone(),
                    },
                ));
            }
        }
        // Also check if an earlier `non_via_move` read in the same statement
        // preceded a `via_move` transfer of the same binding/alias/share group.
        for u in uses {
            if !u.via_move {
                let root = resolve_alias(st, &u.name);
                if let Some((mv_name, _, mv_info)) = stmt_moves.iter().find(|(mv_name, r, mv)| {
                    same_or_shared(st, &u.name, &root, mv_name, r) && mv.span != u.span
                }) {
                    if self
                        .lookup_binding(st, mv_name)
                        .map(|b| !b.shape.is_sendable_by_type())
                        .unwrap_or(false)
                        && reported.insert(format!("{}@{}", u.name, u.span.start.line))
                    {
                        self.emit_use_after_move(st, u, mv_info);
                    }
                }
            }
        }
    }

    /// Check for additional uses in the same statement when a `CallMode::Consume`
    /// call (`enc.finish()`, `flate2.finish(enc)`) consumed a handle during this
    /// statement (`a, b := enc.finish()?, enc.write(...)`, `a, b := enc.finish()?, enc.finish()?`,
    /// `enc.write(enc.finish()?)`).
    pub(crate) fn check_same_stmt_consumes(
        &mut self,
        st: &FnState,
        uses: &[Use],
        stmt_consumes: &[(hy_diag::Span, hy_diag::Span)],
    ) {
        let mut reported: HashSet<(String, u32, u32)> = HashSet::new();
        for u in uses {
            let Some(mv) = self.lookup_binding(st, &u.name).and_then(|b| b.moved.clone()) else {
                continue;
            };
            if mv.kind != MoveKind::Consumed {
                continue;
            }
            let Some(&(consumed_ident_span, _)) = stmt_consumes
                .iter()
                .find(|&&(_, call_span)| call_span == mv.span)
            else {
                continue;
            };
            if u.span == consumed_ident_span {
                continue;
            }
            if reported.insert((u.name.clone(), u.span.start.line, u.span.start.col)) {
                self.emit_use_after_move(st, u, &mv);
            }
        }
    }

    fn emit_use_after_move(&mut self, st: &FnState, u: &Use, mv: &MoveInfo) {
        // A may-share peer (#27) is an alias in every way that matters here,
        // even when the single-parent `aliases` chain does not name it (e.g.
        // a projection or composite-literal capture).
        let is_alias =
            st.aliases.contains_key(&u.name) || st.may_share.contains_key(&u.name);
        let orig = resolve_alias(st, &u.name);
        // Name the binding whose ownership actually left. `orig` is the root of
        // the USED binding, which is the wrong name when the use is a may-share
        // peer of the moved value rather than a member of its alias chain (a
        // reference-typed field or element read out earlier): there `orig`
        // resolved to the peer itself, so the notes named the wrong binding and
        // reported it as aliasing itself.
        let moved_label = if mv.moved_name.is_empty() {
            orig.clone()
        } else {
            mv.moved_name.clone()
        };
        let span_str = format!("{}:{}", mv.span.start.line, mv.span.start.col);

        let msg = if u.via_move {
            format!("`{}` is moved again after ownership already left", u.name)
        } else {
            match (&mv.kind, u.is_write, is_alias) {
                (MoveKind::Consumed, _, _) => {
                    format!("`{}` is used after ownership was consumed", u.name)
                }
                (_, true, true) => "write through a live alias of a moved value".to_string(),
                (_, true, false) => {
                    format!("`{}` is used after ownership was moved away", u.name)
                }
                (_, false, true) => {
                    "frozen alias is read after the value was moved away".to_string()
                }
                (_, false, false) => {
                    format!("`{}` is read after ownership was moved away", u.name)
                }
            }
        };

        let mut notes = Vec::new();
        if mv.kind == MoveKind::Consumed {
            notes.push(format!(
                "ownership of `{}` was transferred by {} (marked `consume` in the boundary descriptor), {}:{}",
                moved_label, mv.desc, self.path, span_str
            ));
            notes.push(
                "the transfer site is reported above so you can see where ownership left — not just the line that noticed".to_string(),
            );
            if is_alias && u.name != moved_label {
                notes.push(format!(
                    "`{}` aliases `{}` — the consume freeze covers every live alias too (O13)",
                    u.name, moved_label
                ));
            }
        } else {
            notes.push(format!(
                "ownership of `{}` left this task at {} ({}:{})",
                moved_label, mv.desc, self.path, span_str
            ));
            if is_alias && u.name != moved_label {
                notes.push(format!(
                    "`{}` aliases `{}` — the freeze covers every live alias, otherwise the alias binding would defeat it entirely (O5.2)",
                    u.name, moved_label
                ));
                if u.is_write {
                    notes.push(
                        "this is a WRITE through an alias — rejected under every reading of the freeze rule (the READ form's contested status is corpus 19)".to_string(),
                    );
                }
            } else {
                notes.push(
                    "after a move the sender — and every live alias — must not use the value again (O5.2)".to_string(),
                );
                if !u.is_write {
                    notes.push(
                        "reads are included deliberately: the receiver may mutate concurrently (Go DRF-SC: concurrent unsynchronized read/write is a race). the strict form is provisional — open decision D1, pre-registered for human review at G1 (corpus program 19)".to_string(),
                    );
                }
                notes.push(
                    "over GC memory this cannot dangle — the rule is the single-logical-owner discipline, not memory safety".to_string(),
                );
            }
        }

        let mut d =
            Diagnostic::error("HYB-E3003", self.path.clone(), u.span, msg).with_notes(notes);
        if mv.kind == MoveKind::Consumed {
            d = d.with_rule("A7");
        }
        self.diags.push(d);
    }

    // -----------------------------------------------------------------------
    // Move marking + alias freezing (O5.2)
    // -----------------------------------------------------------------------

    /// Mark `name` moved-from at the transfer site and freeze every live
    /// alias in its alias group (both directions: an alias of the moved
    /// binding, and the original when an alias is the one moved — they
    /// denote the same reference-typed value).
    pub(crate) fn mark_move(
        &self,
        st: &mut FnState,
        name: &str,
        span: hy_diag::Span,
        desc: String,
        kind: MoveKind,
    ) {
        // #27 / #131: a move freezes every binding in the transitive connected
        // component of `name` across both `aliases` (parent and child
        // directions) and `may_share` — including multi-hop chains such as a
        // pointer borrow embedded in a composite literal (`p := &h; xs := []*holder{p}`).
        let mut members: Vec<String> = storage_component(st, name).into_iter().collect();
        members.sort();
        let mut origins = Vec::new();
        for m in &members {
            let o = self.non_owned_origin_of(st, m).map(|(orig, via_share)| {
                if via_share && !orig.starts_with("a may-share peer of ") {
                    format!("a may-share peer of {}", orig)
                } else {
                    orig
                }
            });
            origins.push(o);
        }
        let mut shadow_targets: Vec<(String, usize)> = Vec::new();
        for m in &members {
            if let Some(links) = st.shadow_links.get(m) {
                shadow_targets.extend(links.iter().cloned());
            }
        }
        for (m, orig) in members.iter().zip(origins) {
            if let Some(b) = st.env_lookup_mut(m) {
                if b.non_owned_origin.is_none() {
                    b.non_owned_origin = orig;
                }
                b.moved = Some(MoveInfo {
                    span,
                    kind,
                    desc: desc.clone(),
                    moved_name: name.to_string(),
                });
            } else if let Some(cb) = self.consts.get(m).cloned() {
                if !cb.shape.is_sendable_by_type() {
                    let mut frozen = cb;
                    frozen.moved = Some(MoveInfo {
                        span,
                        kind,
                        desc: desc.clone(),
                        moved_name: name.to_string(),
                    });
                    st.env.insert(m.clone(), frozen);
                }
            }
        }
        for (outer_name, depth) in shadow_targets {
            if let Some(b) = st.binding_at_depth_mut(&outer_name, depth) {
                if b.moved.is_none() {
                    b.moved = Some(MoveInfo {
                        span,
                        kind,
                        desc: desc.clone(),
                        moved_name: name.to_string(),
                    });
                }
            }
        }
    }

    /// Process a place expression (assignment target) that may carry a move
    /// transfer inside its base or index expression (`(move m)["k"] = 1`,
    /// `m[idx(move x)] = 1`).
    pub(crate) fn mark_move_in_place(
        &mut self,
        st: &mut FnState,
        place: &hy_syntax::Place,
    ) {
        match &place.kind {
            hy_syntax::PlaceKind::Ident(_) => {}
            hy_syntax::PlaceKind::Field { base, .. }
            | hy_syntax::PlaceKind::AddrOf { base } => {
                self.mark_move_in_place(st, base);
            }
            hy_syntax::PlaceKind::Index { base, index } => {
                self.mark_move_in_place(st, base);
                self.mark_move_in_value(
                    st,
                    index,
                    format!("the index expression `{}`", index.render()),
                );
            }
        }
    }

    /// Process an expression that may carry a move transfer at a
    /// define/assign/return site (`x := move y`, `x = move y`,
    /// `return move y`), freezing moved bindings (O5.1) and rejecting
    /// moves out of GC fields with HYB-E3004 (O5.4).
    pub(crate) fn mark_move_in_value(&mut self, st: &mut FnState, value: &Expr, desc: String) {
        let mut ops = Vec::new();
        crate::model::collect_move_operands(value, &mut ops);
        for (op, mspan) in ops {
            if let Expr::Ident { name, .. } = op {
                self.mark_move(st, name, mspan, desc.clone(), MoveKind::Moved);
            } else if matches!(op, Expr::Field { .. })
                && matches!(
                    self.shape_of_expr(st, op),
                    crate::model::TypeShape::OwnHandle(_)
                )
            {
                if let Some(root) = op.root_ident() {
                    self.mark_move(st, root, mspan, desc.clone(), MoveKind::Moved);
                }
            } else {
                self.check_o54_non_ident_move(st, op, mspan);
            }
        }
    }

    /// Resolve any non-owned origin (direct or via may-share peers) among the
    /// `move` operands embedded in `value`.
    pub(crate) fn moved_non_owned_origin_in_expr(
        &self,
        st: &FnState,
        value: &Expr,
    ) -> Option<String> {
        let mut ops = Vec::new();
        crate::model::collect_move_operands(value, &mut ops);
        for (op, _) in ops {
            let root = match op {
                Expr::Ident { name, .. } => Some(name.as_str()),
                Expr::Field { .. } => op.root_ident(),
                _ => None,
            };
            if let Some(src) = root {
                if let Some((o, via_share)) = self.non_owned_origin_of(st, src) {
                    return Some(if via_share {
                        format!("a may-share peer of {}", o)
                    } else {
                        o
                    });
                }
            }
        }
        None
    }

    /// O5.4 move-form check (HYB-E3004): `move` applies to whole bindings and
    /// own-handle fields (`move s.conn`), never GC composite fields (`move s.data`).
    pub(crate) fn check_o54_non_ident_move(
        &mut self,
        st: &FnState,
        operand: &Expr,
        span: hy_diag::Span,
    ) {
        let shape = self.shape_of_expr(st, operand);
        if matches!(operand, Expr::Field { .. })
            && matches!(shape, crate::model::TypeShape::OwnHandle(_))
        {
            return;
        }
        let parent = operand.root_ident().unwrap_or("<value>");
        let msg = format!(
            "cannot move `{}` ({}) — `move` applies to whole bindings and own-handle fields, not GC fields (O5.4)",
            operand.render(),
            shape.render()
        );
        let notes = vec![
            format!(
                "why: `{}` still aliases the underlying storage after `move {}`, so freezing the field expression cannot prevent concurrent access through `{}` (O5.4)",
                parent,
                operand.render(),
                parent
            ),
            format!(
                "fix: move the whole binding (`move {}`) so it and every live alias are frozen, or copy `{}` into a fresh local before transferring",
                parent,
                operand.render()
            ),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3004", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// O5.1 / O5.2 / O5.4: `move x` at a concurrency boundary requires that `x`
    /// be an exclusively owned local binding so freezing `x` (and its local
    /// aliases) freezes every live reference in the sending task. A function/
    /// closure parameter or receiver, an outer binding captured by a closure,
    /// a range variable over a non-channel collection, or a `const` composite
    /// (#125) remains aliased outside the move site, so moving it is rejected
    /// with HYB-E3004 (`tier0_rule: "A1b"`).
    pub(crate) fn check_non_owned_move(
        &mut self,
        st: &FnState,
        name: &str,
        span: hy_diag::Span,
    ) {
        let Some(b) = self.lookup_binding(st, name) else {
            return;
        };
        if b.shape.is_sendable_by_type() || b.managed_taint {
            return;
        }
        let Some((origin, via_share)) = self.non_owned_origin_of(st, name) else {
            return;
        };
        let ty_str = b
            .ty
            .as_ref()
            .map(|t| t.render())
            .unwrap_or_else(|| b.shape.render());
        let msg = if via_share {
            format!(
                "cannot move `{}` ({}) — `move` requires an exclusively owned local binding, not a may-share peer of {} (O5.1 / O5.4)",
                name, ty_str, origin
            )
        } else {
            format!(
                "cannot move `{}` ({}) — `move` requires an exclusively owned local binding, not {} (O5.1 / O5.4)",
                name, ty_str, origin
            )
        };
        let why = if via_share {
            format!(
                "why: `{}` may share mutable GC storage with {}; the sending frame's freeze of `{}` cannot reach that live alias (O5.2 / O5.4)",
                name, origin, name
            )
        } else {
            format!(
                "why: {} still aliases the underlying storage outside this move site, so freezing `{}` cannot prevent concurrent access (O5.2 / O5.4)",
                origin, name
            )
        };
        let notes = vec![
            why,
            format!(
                "fix: copy `{}` into a fresh local before moving (`fresh := ...; move fresh`), move the caller's owned binding at the spawn/send site directly, or wrap a genuinely synchronized share in `managed(\"reason\", \"category\") {{ ... }}`",
                name
            ),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3004", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// The non-owned origin `check_non_owned_move` would cite for `name`:
    /// `(origin, via_share)`. Also read by `process_local` BEFORE a
    /// self-shadowing define (`m := m`) replaces the visible binding.
    pub(crate) fn non_owned_origin_of(&self, st: &FnState, name: &str) -> Option<(String, bool)> {
        let b = self.lookup_binding(st, name)?;
        let root = resolve_alias(st, name);
        // (a) the binding's own non-owned origin (param / receiver /
        //     captured / range var), or its alias-chain root's.
        let own = b.non_owned_origin.clone().or_else(|| {
            self.lookup_binding(st, &root)
                .and_then(|rb| rb.non_owned_origin.clone())
        });
        // (b) issue #75: failing (a), any non-owned origin in the binding's
        //     MAY-SHARE component (or the root's — pr-80's union). `h2 := h`
        //     (shallow struct/array copy), a re-slice, a projection, or a
        //     literal capture records a share edge but no define-time origin
        //     inheritance (that path is reference-alias shapes only), so a
        //     local peer of a parameter used to pass this check — while the
        //     caller still holds a live binding to the same storage, exactly
        //     the situation moving the parameter itself is rejected for.
        //     Peers are scanned in sorted order for deterministic
        //     diagnostics.
        let mut origin: Option<(String, bool)> = own.map(|o| {
            if let Some(rest) = o.strip_prefix("a may-share peer of ") {
                (rest.to_string(), true)
            } else {
                (o, false)
            }
        });
        if origin.is_none() {
            let mut peers: Vec<String> = storage_component(st, name).into_iter().collect();
            peers.sort();
            for peer in peers {
                if peer == name {
                    continue;
                }
                let proot = resolve_alias(st, &peer);
                let po = self
                    .lookup_binding(st, &peer)
                    .and_then(|pb| pb.non_owned_origin.clone())
                    .or_else(|| {
                        self.lookup_binding(st, &proot)
                            .and_then(|rb| rb.non_owned_origin.clone())
                    });
                if let Some(po) = po {
                    let clean = po
                        .strip_prefix("a may-share peer of ")
                        .unwrap_or(&po)
                        .to_string();
                    origin = Some((clean, true));
                    break;
                }
            }
        }
        origin
    }

    /// O5.1 / O5.2 / O13 across loop iterations: when a loop body can repeat,
    /// any outer binding that was moved or consumed inside the loop and remains
    /// moved at the end of the body would be used or moved again on the next
    /// iteration. Walk the loop's statements along the back-edge with those
    /// loop-carried moves active; local definitions (`chunk := ...`, corpus 15)
    /// live in inner scopes and are discarded when `scopes` is truncated to
    /// `outer_depth`.
    fn is_single_iteration_loop(for_stmt: Option<&hy_syntax::Stmt>) -> bool {
        let Some(hy_syntax::Stmt::For { kind, .. }) = for_stmt else {
            return false;
        };
        if let hy_syntax::ForKind::Range { lo, hi, inclusive, .. } = kind {
            if let (Some(lo_val), Some(hi_val)) = (Self::as_int_lit(lo), Self::as_int_lit(hi)) {
                let count = if *inclusive {
                    if hi_val >= lo_val {
                        hi_val.checked_sub(lo_val).and_then(|d| d.checked_add(1))
                    } else {
                        Some(0)
                    }
                } else {
                    if hi_val > lo_val {
                        hi_val.checked_sub(lo_val)
                    } else {
                        Some(0)
                    }
                };
                if let Some(c) = count {
                    if c <= 1 {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn as_int_lit(expr: &hy_syntax::Expr) -> Option<i64> {
        match expr {
            hy_syntax::Expr::Lit {
                kind: hy_syntax::LitKind::Int(s),
                ..
            } => s.parse::<i64>().ok(),
            hy_syntax::Expr::Unary {
                op: hy_syntax::UnOp::Neg,
                operand,
                ..
            } => Self::as_int_lit(operand).and_then(|v| v.checked_neg()),
            _ => None,
        }
    }

    fn check_loop_backedge_replay(
        &mut self,
        iter_st: &FnState,
        loop_stmts: &[(&hy_syntax::Stmt, usize)],
        events: &std::collections::HashMap<usize, Vec<crate::walk::ScopeEvent>>,
        in_managed: &dyn Fn(usize) -> bool,
    ) {
        let has_loop_carried_state = iter_st
            .env
            .iter()
            .chain(iter_st.scopes.iter().flat_map(|s| s.iter()))
            .any(|(name, b)| {
                b.moved.is_some()
                    || self.non_owned_origin_of(iter_st, name).is_some()
                    || iter_st.aliases.contains_key(name)
                    || !iter_st.share_group(name).is_empty()
            });
        if !has_loop_carried_state {
            return;
        }

        let mut replay_st = iter_st.clone();
        let diags_len_before = self.diags.len();
        let managed_blocks_before = self.managed_blocks;
        let sites_via_managed_before = self.sites_via_managed;
        let total_sites_before = self.total_sites;
        let managed_audit_len_before = self.managed_audit.len();

        for (i, (stmt, stmt_idx)) in loop_stmts.iter().enumerate() {
            let idx = *stmt_idx;
            let managed = in_managed(idx);

            if let Some(evs) = events.get(&idx) {
                for ev in evs {
                    match ev {
                        crate::walk::ScopeEvent::Push => replay_st.scope_push(),
                        crate::walk::ScopeEvent::Pop => replay_st.scope_pop(),
                        crate::walk::ScopeEvent::SelectRecv { names, chan } => {
                            let elem = match self.shape_of_expr(&replay_st, chan) {
                                crate::model::TypeShape::Chan(e) => *e,
                                _ => crate::model::TypeShape::Unknown,
                            };
                            for (j, n) in names.iter().enumerate() {
                                let shape = if j == 0 {
                                    elem.clone()
                                } else {
                                    crate::model::TypeShape::Primitive
                                };
                                self.define_binding(
                                    &mut replay_st,
                                    n,
                                    shape,
                                    None,
                                    false,
                                    managed,
                                    false,
                                );
                            }
                        }
                        crate::walk::ScopeEvent::MatchBind { name, subject } => {
                            self.bind_match_arm(&mut replay_st, name, subject, managed);
                        }
                    }
                }
            }

            if i == 0 {
                if let hy_syntax::Stmt::For { kind, .. } = stmt {
                    match kind {
                        hy_syntax::ForKind::Iterate { vars, expr, .. } => {
                            let elem = match self.shape_of_expr(&replay_st, expr) {
                                crate::model::TypeShape::Slice(e) | crate::model::TypeShape::Array(e) => *e,
                                crate::model::TypeShape::Chan(e) => *e,
                                crate::model::TypeShape::Map(_, v) => *v,
                                _ => crate::model::TypeShape::Unknown,
                            };
                            for (idx_var, v) in vars.iter().enumerate() {
                                let shape = if idx_var == 0 && vars.len() > 1 {
                                    crate::model::TypeShape::Primitive
                                } else {
                                    elem.clone()
                                };
                                self.define_binding(&mut replay_st, v, shape, None, false, managed, false);
                            }
                        }
                        hy_syntax::ForKind::Range { var, .. } => {
                            self.define_binding(
                                &mut replay_st,
                                var,
                                crate::model::TypeShape::Primitive,
                                None,
                                false,
                                managed,
                                false,
                            );
                        }
                        _ => {}
                    }
                }
                continue;
            }

            let mut uses = Vec::new();
            crate::walk::own_uses_with_results(stmt, &replay_st.named_results, &mut uses);
            self.check_uses(&replay_st, &uses);

            match stmt {
                hy_syntax::Stmt::Local(l) => {
                    self.process_local(&mut replay_st, l, idx, managed);
                    self.record_local_borrow_aliases(&mut replay_st, l);
                    self.mark_slot_loans_local(&mut replay_st, l);
                }
                hy_syntax::Stmt::Assign {
                    targets,
                    op,
                    values,
                    span,
                } => {
                    self.process_assign(&mut replay_st, targets, *op, values, *span, idx);
                    self.record_assign_borrow_aliases(&mut replay_st, targets, *op, values);
                    self.mark_slot_loans_assign(&mut replay_st, targets, *op, values);
                }
                hy_syntax::Stmt::Task { callee, args, span } => {
                    self.mark_move_in_value(
                        &mut replay_st,
                        callee,
                        format!("the spawn callee `{}`", callee.render()),
                    );
                    let desc = format!("the spawn `task {}(...)`", callee.render());
                    let vals: Vec<&hy_syntax::Expr> = args.iter().map(|a| &a.value).collect();
                    let targets = self.spawn_param_shapes(&replay_st, callee, args.len());
                    self.check_site(
                        &mut replay_st,
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
                hy_syntax::Stmt::Send {
                    chan,
                    value,
                    is_move,
                    span,
                } => {
                    self.mark_move_in_value(
                        &mut replay_st,
                        chan,
                        format!("the send target `{}`", chan.render()),
                    );
                    if self.check_send_target(&replay_st, chan, *span) {
                        let desc = if *is_move {
                            format!(
                                "the move-send `{} <- move {}`",
                                chan.render(),
                                value.render()
                            )
                        } else {
                            format!("the send `{} <- {}`", chan.render(), value.render())
                        };
                        self.check_site(
                            &mut replay_st,
                            *span,
                            "HYB-E3002",
                            &[value],
                            managed,
                            &desc,
                            *is_move,
                            None,
                            None,
                        );
                    }
                }
                hy_syntax::Stmt::Select { arms, .. } => {
                    for arm in arms {
                        if let hy_syntax::SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                            ..
                        } = &arm.kind
                        {
                            self.mark_move_in_value(
                                &mut replay_st,
                                chan,
                                format!("the select-send target `{}`", chan.render()),
                            );
                            let span = chan.span().to(value.span());
                            if self.check_send_target(&replay_st, chan, span) {
                                let desc = if *is_move {
                                    format!(
                                        "the move-send `{} <- move {}`",
                                        chan.render(),
                                        value.render()
                                    )
                                } else {
                                    format!(
                                        "the select-send `{} <- {}`",
                                        chan.render(),
                                        value.render()
                                    )
                                };
                                let vals: Vec<&hy_syntax::Expr> = vec![value];
                                let targets = self.chan_elem_shape(&replay_st, chan);
                                self.check_site(
                                    &mut replay_st,
                                    span,
                                    "HYB-E3002",
                                    &vals,
                                    managed,
                                    &desc,
                                    *is_move,
                                    None,
                                    targets.as_ref().map(|v| &**v),
                                );
                            }
                        }
                    }
                }
                hy_syntax::Stmt::Defer { call, .. } => {
                    self.mark_move_in_value(
                        &mut replay_st,
                        call,
                        format!("the deferred call `defer {}`", call.render()),
                    );
                }
                _ => {}
            }
        }

        self.managed_blocks = managed_blocks_before;
        self.sites_via_managed = sites_via_managed_before;
        self.total_sites = total_sites_before;
        self.managed_audit.truncate(managed_audit_len_before);

        let new_diags = self.diags.split_off(diags_len_before);
        for d in new_diags {
            if d.code == "HYB-E3003" || d.code == "HYB-E3004" {
                if !self
                    .diags
                    .iter()
                    .any(|existing| existing.code == d.code && existing.span == d.span)
                {
                    self.diags.push(d);
                }
            }
        }
    }

    pub(crate) fn check_loop_backedge(
        &mut self,
        st: &FnState,
        outer_depth: usize,
        loop_stmts: &[(&hy_syntax::Stmt, usize)],
        events: &std::collections::HashMap<usize, Vec<crate::walk::ScopeEvent>>,
        loop_body: &hy_syntax::Block,
        pre_moved: &HashSet<String>,
        reported_backedge: &mut HashSet<String>,
        in_managed: &dyn Fn(usize) -> bool,
    ) {
        if matches!(
            loop_body.stmts.last(),
            Some(hy_syntax::Stmt::Break { .. } | hy_syntax::Stmt::Return { .. })
        ) && !Self::has_continue_after_loop_move(loop_body, st, pre_moved, outer_depth)
        {
            return;
        }

        if Self::is_single_iteration_loop(loop_stmts.first().map(|(s, _)| *s)) {
            return;
        }

        let mut iter_st = st.clone();
        while iter_st.scopes.len() > outer_depth {
            iter_st.scope_pop();
        }

        self.check_loop_backedge_replay(&iter_st, loop_stmts, events, in_managed);

        let popped_st = iter_st.clone();
        let mut any_loop_carried = false;
        for (name, b) in iter_st
            .env
            .iter_mut()
            .chain(iter_st.scopes.iter_mut().flat_map(|s| s.iter_mut()))
        {
            if b.moved.is_some() {
                let root = resolve_alias(&popped_st, name);
                if pre_moved.contains(name)
                    || reported_backedge.contains(&root)
                {
                    b.moved = None;
                } else {
                    any_loop_carried = true;
                }
            }
        }
        if !any_loop_carried {
            return;
        }

        let mut shadow_scopes: Vec<HashSet<String>> = vec![HashSet::new()];

        for (i, (stmt, stmt_idx)) in loop_stmts.iter().enumerate() {
            if i > 0 {
                if let Some(evs) = events.get(stmt_idx) {
                    for ev in evs {
                        match ev {
                            crate::walk::ScopeEvent::Push => shadow_scopes.push(HashSet::new()),
                            crate::walk::ScopeEvent::Pop => {
                                if shadow_scopes.len() > 1 {
                                    shadow_scopes.pop();
                                }
                            }
                            crate::walk::ScopeEvent::SelectRecv { names, .. } => {
                                if let Some(top) = shadow_scopes.last_mut() {
                                    for n in names.iter() {
                                        top.insert(n.clone());
                                    }
                                }
                            }
                            crate::walk::ScopeEvent::MatchBind { name, .. } => {
                                if let Some(top) = shadow_scopes.last_mut() {
                                    top.insert(name.to_string());
                                }
                            }
                        }
                    }
                }
            }

            let mut uses = Vec::new();
            if i == 0 {
                match stmt {
                    hy_syntax::Stmt::For {
                        kind: hy_syntax::ForKind::While { cond },
                        ..
                    } => {
                        crate::walk::expr_uses(cond, &mut uses);
                    }
                    hy_syntax::Stmt::For {
                        kind: hy_syntax::ForKind::Iterate { expr, .. },
                        ..
                    } => {
                        crate::walk::expr_uses(expr, &mut uses);
                    }
                    hy_syntax::Stmt::For {
                        kind: hy_syntax::ForKind::Range { hi, .. },
                        ..
                    } => {
                        // Integration note (21-b): the PR also added `lo`
                        // here, but the emitted Go evaluates `lo` ONCE at
                        // loop init (`for v := lo; v < hi; v++`) — and the
                        // inclusive form hoists both bounds into hyLo/hyHi
                        // temps — so `lo` is never re-read on the back-edge.
                        // A move inside the body cannot race the header's
                        // `lo`; keeping the hunk would model a read that
                        // does not happen (and re-order first-diagnostics
                        // in already-rejecting shapes). `hi` IS the loop
                        // condition and stays checked.
                        crate::walk::expr_uses(hi, &mut uses);
                    }
                    _ => {}
                }
            } else {
                crate::walk::own_uses(stmt, &mut uses);
                if let hy_syntax::Stmt::Select { arms, .. } = stmt {
                    for arm in arms {
                        match &arm.kind {
                            hy_syntax::SelectArmKind::Recv { chan, .. }
                            | hy_syntax::SelectArmKind::Send { chan, .. } => {
                                crate::walk::expr_uses(chan, &mut uses);
                            }
                            hy_syntax::SelectArmKind::Default => {}
                        }
                    }
                }
            }

            for u in &uses {
                if shadow_scopes.iter().any(|s| s.contains(&u.name)) {
                    continue;
                }
                let moved = match self.lookup_binding(&iter_st, &u.name) {
                    Some(b) => b.moved.clone(),
                    None => continue,
                };
                let Some(mv) = moved else { continue };
                self.emit_use_after_move(&iter_st, u, &mv);
                let root = resolve_alias(&iter_st, &u.name);
                reported_backedge.insert(root.clone());
                let mut clear_names = vec![u.name.clone(), root.clone()];
                for a in iter_st.aliases.keys() {
                    if resolve_alias(&iter_st, a) == root {
                        clear_names.push(a.clone());
                    }
                }
                let seeded = clear_names.clone();
                for m in seeded {
                    clear_names.extend(iter_st.share_group(&m));
                }
                for k in &clear_names {
                    if let Some(b) = iter_st.env_lookup_mut(k) {
                        b.moved = None;
                    }
                }
            }

            match stmt {
                hy_syntax::Stmt::Local(l) if i > 0 => {
                    if let Some(top) = shadow_scopes.last_mut() {
                        for name in &l.names {
                            top.insert(name.clone());
                        }
                    }
                }
                hy_syntax::Stmt::For { kind, .. } => {
                    if let Some(top) = shadow_scopes.last_mut() {
                        match kind {
                            hy_syntax::ForKind::Range { var, .. } => {
                                top.insert(var.clone());
                            }
                            hy_syntax::ForKind::Iterate { vars, .. } => {
                                for v in vars {
                                    top.insert(v.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn has_continue_after_loop_move(
        loop_body: &hy_syntax::Block,
        st: &FnState,
        pre_moved: &HashSet<String>,
        outer_depth: usize,
    ) -> bool {
        if !block_has_continue_for_loop(loop_body) {
            return false;
        }
        let mut outer = st.clone();
        while outer.scopes.len() > outer_depth {
            outer.scope_pop();
        }
        outer.env.iter().chain(outer.scopes.iter().flat_map(|s| s.iter())).any(|(name, b)| {
            !pre_moved.contains(name)
                && b.moved.as_ref().is_some_and(|mv| Self::move_span_can_reach_backedge(loop_body, mv.span))
        })
    }

    fn move_span_can_reach_backedge(
        loop_body: &hy_syntax::Block,
        move_span: hy_diag::Span,
    ) -> bool {
        let top_flow = LoopFlow {
            continue_reach: true,
            loop_break_reach: false,
            select_break_reach: None,
        };
        Self::block_move_can_reach_backedge(loop_body, move_span, true, top_flow)
            .unwrap_or_else(|| Self::block_can_reach_backedge(loop_body, true, top_flow))
    }

    fn block_can_reach_backedge(
        block: &hy_syntax::Block,
        tail_reach: bool,
        flow: LoopFlow,
    ) -> bool {
        let mut cur_after = tail_reach;
        for stmt in block.stmts.iter().rev() {
            cur_after = Self::stmt_can_reach_backedge(stmt, cur_after, flow);
        }
        cur_after
    }

    fn stmt_can_reach_backedge(
        stmt: &hy_syntax::Stmt,
        after_reach: bool,
        flow: LoopFlow,
    ) -> bool {
        match stmt {
            hy_syntax::Stmt::Return { .. } => false,
            hy_syntax::Stmt::Continue { .. } => flow.continue_reach,
            hy_syntax::Stmt::Break {
                label: Some(_), ..
            } => flow.loop_break_reach,
            hy_syntax::Stmt::Break { label: None, .. } => {
                flow.select_break_reach.unwrap_or(flow.loop_break_reach)
            }
            hy_syntax::Stmt::Block { block, .. }
            | hy_syntax::Stmt::Managed { body: block, .. } => {
                Self::block_can_reach_backedge(block, after_reach, flow)
            }
            hy_syntax::Stmt::If {
                then, otherwise, ..
            } => {
                let then_reach = Self::block_can_reach_backedge(then, after_reach, flow);
                let else_reach = match otherwise.as_deref() {
                    Some(o) => Self::stmt_can_reach_backedge(o, after_reach, flow),
                    None => after_reach,
                };
                then_reach || else_reach
            }
            hy_syntax::Stmt::Match { arms, .. } => {
                if arms.is_empty() {
                    after_reach
                } else {
                    arms.iter()
                        .any(|a| Self::block_can_reach_backedge(&a.body, after_reach, flow))
                        || (arms.iter().all(|a| a.guard.is_some()) && after_reach)
                }
            }
            hy_syntax::Stmt::Select { arms, .. } => {
                let sel_flow = LoopFlow {
                    select_break_reach: Some(after_reach),
                    ..flow
                };
                arms.iter()
                    .any(|a| Self::block_can_reach_backedge(&a.body, after_reach, sel_flow))
            }
            hy_syntax::Stmt::For { kind, body, .. } => {
                if matches!(kind, hy_syntax::ForKind::Infinite) {
                    let inner_flow = LoopFlow {
                        continue_reach: false,
                        loop_break_reach: after_reach,
                        select_break_reach: None,
                    };
                    Self::block_can_reach_backedge(body, false, inner_flow)
                } else {
                    after_reach
                }
            }
            hy_syntax::Stmt::Local(_)
            | hy_syntax::Stmt::Assign { .. }
            | hy_syntax::Stmt::Expr { .. }
            | hy_syntax::Stmt::Send { .. }
            | hy_syntax::Stmt::Task { .. }
            | hy_syntax::Stmt::Defer { .. } => after_reach,
        }
    }

    fn block_move_can_reach_backedge(
        block: &hy_syntax::Block,
        move_span: hy_diag::Span,
        tail_reach: bool,
        flow: LoopFlow,
    ) -> Option<bool> {
        if !Self::span_contains(block.span, move_span) {
            return None;
        }
        let mut cur_after = tail_reach;
        for stmt in block.stmts.iter().rev() {
            if let Some(res) = Self::stmt_move_can_reach_backedge(stmt, move_span, cur_after, flow)
            {
                return Some(res);
            }
            cur_after = Self::stmt_can_reach_backedge(stmt, cur_after, flow);
        }
        None
    }

    fn stmt_move_can_reach_backedge(
        stmt: &hy_syntax::Stmt,
        move_span: hy_diag::Span,
        after_reach: bool,
        flow: LoopFlow,
    ) -> Option<bool> {
        if !Self::span_contains(stmt.span(), move_span) {
            return None;
        }
        match stmt {
            hy_syntax::Stmt::Return { .. } => Some(false),
            hy_syntax::Stmt::Continue { .. } => Some(flow.continue_reach),
            hy_syntax::Stmt::Break {
                label: Some(_), ..
            } => Some(flow.loop_break_reach),
            hy_syntax::Stmt::Break { label: None, .. } => {
                Some(flow.select_break_reach.unwrap_or(flow.loop_break_reach))
            }
            hy_syntax::Stmt::Block { block, .. }
            | hy_syntax::Stmt::Managed { body: block, .. } => {
                Self::block_move_can_reach_backedge(block, move_span, after_reach, flow)
            }
            hy_syntax::Stmt::If {
                then, otherwise, ..
            } => {
                if let Some(res) =
                    Self::block_move_can_reach_backedge(then, move_span, after_reach, flow)
                {
                    return Some(res);
                }
                if let Some(o) = otherwise.as_deref() {
                    if let Some(res) =
                        Self::stmt_move_can_reach_backedge(o, move_span, after_reach, flow)
                    {
                        return Some(res);
                    }
                }
                Some(Self::stmt_can_reach_backedge(stmt, after_reach, flow))
            }
            hy_syntax::Stmt::Match { arms, .. } => {
                for arm in arms {
                    if let Some(res) =
                        Self::block_move_can_reach_backedge(&arm.body, move_span, after_reach, flow)
                    {
                        return Some(res);
                    }
                }
                Some(Self::stmt_can_reach_backedge(stmt, after_reach, flow))
            }
            hy_syntax::Stmt::Select { arms, .. } => {
                let sel_flow = LoopFlow {
                    select_break_reach: Some(after_reach),
                    ..flow
                };
                for arm in arms {
                    if let Some(res) = Self::block_move_can_reach_backedge(
                        &arm.body,
                        move_span,
                        after_reach,
                        sel_flow,
                    ) {
                        return Some(res);
                    }
                }
                // Channel and send-value expressions in `select` arm headers are
                // evaluated before an arm is chosen, so a move/consume in a
                // header reaches the loop back-edge whenever ANY arm of the
                // `select` can reach the back-edge.
                Some(Self::stmt_can_reach_backedge(stmt, after_reach, flow))
            }
            hy_syntax::Stmt::For { body, .. } => {
                let loop_exits = Self::stmt_can_reach_backedge(stmt, after_reach, flow);
                let inner_flow = LoopFlow {
                    continue_reach: loop_exits,
                    loop_break_reach: after_reach,
                    select_break_reach: None,
                };
                if let Some(res) =
                    Self::block_move_can_reach_backedge(body, move_span, loop_exits, inner_flow)
                {
                    return Some(res);
                }
                Some(loop_exits)
            }
            hy_syntax::Stmt::Local(_)
            | hy_syntax::Stmt::Assign { .. }
            | hy_syntax::Stmt::Expr { .. }
            | hy_syntax::Stmt::Send { .. }
            | hy_syntax::Stmt::Task { .. }
            | hy_syntax::Stmt::Defer { .. } => Some(after_reach),
        }
    }

    fn span_contains(outer: hy_diag::Span, inner: hy_diag::Span) -> bool {
        (outer.start.line, outer.start.col) <= (inner.start.line, inner.start.col)
            && (inner.end.line, inner.end.col) <= (outer.end.line, outer.end.col)
    }

    /// True only when `expr` calls a file-local function whose return statements
    /// all construct a fresh value-element slice via `append([]T{}, ...)` (such
    /// as corpus 15's `chunk_of`), so the returned slice cannot share mutable
    /// backing storage with the caller's argument (#194).
    pub(crate) fn call_returns_fresh_copy(&self, st: &FnState, expr: &Expr) -> bool {
        let Expr::Call { callee, .. } = crate::model::unwrap_parens(expr) else {
            return false;
        };
        let Expr::Ident { name, .. } = crate::model::unwrap_parens(callee) else {
            return false;
        };
        if self.lookup_binding(st, name).is_some() {
            return false;
        }
        let Some(fdecl) = self.fns.get(name.as_str()) else {
            return false;
        };
        if !fdecl.type_params.is_empty()
            || fdecl.results.len() != 1
            || fdecl.results[0].name.is_some()
        {
            return false;
        }
        if self.consts.contains_key("append")
            || self.fns.contains_key("append")
            || self.structs.contains_key("append")
            || self.enums.contains_key("append")
            || self.type_aliases.contains_key("append")
            || self.packages.contains("append")
        {
            return false;
        }
        if fdecl.params.iter().any(|p| p.name == "append") {
            return false;
        }
        let ret_shape = self.shape_of_type(&fdecl.results[0].ty);
        let crate::model::TypeShape::Slice(elem) = ret_shape else {
            return false;
        };
        if !self.shape_is_proven_pure_value(&elem, 0) {
            return false;
        }
        let Some(body) = &fdecl.body else {
            return false;
        };
        let mut saw_return = false;
        if !Self::block_returns_only_fresh_append(body, &mut saw_return) {
            return false;
        }
        saw_return
    }

    fn shape_is_proven_pure_value(&self, shape: &crate::model::TypeShape, depth: usize) -> bool {
        if depth > 16 {
            return false;
        }
        match shape {
            crate::model::TypeShape::Primitive | crate::model::TypeShape::Str => true,
            crate::model::TypeShape::Array(inner) => {
                self.shape_is_proven_pure_value(inner, depth + 1)
            }
            crate::model::TypeShape::Struct(Some(sname)) => {
                if self.file.decls.iter().any(|d| {
                    matches!(
                        d,
                        hy_syntax::Decl::Struct(s)
                            if s.name == *sname && !s.type_params.is_empty()
                    )
                }) {
                    return false;
                }
                let Some(fields) = self.structs.get(sname.as_str()) else {
                    return false;
                };
                fields.iter().all(|(_, fty)| {
                    self.shape_is_proven_pure_value(&self.shape_of_type(fty), depth + 1)
                })
            }
            _ => false,
        }
    }

    fn block_returns_only_fresh_append(block: &hy_syntax::Block, saw_return: &mut bool) -> bool {
        for stmt in &block.stmts {
            match stmt {
                hy_syntax::Stmt::Defer { .. } => {
                    return false;
                }
                hy_syntax::Stmt::Local(l) => {
                    if l.names.iter().any(|n| n == "append") {
                        return false;
                    }
                }
                hy_syntax::Stmt::Return { values, .. } => {
                    *saw_return = true;
                    if values.len() != 1 || !Self::is_fresh_empty_slice_append(&values[0]) {
                        return false;
                    }
                }
                hy_syntax::Stmt::If {
                    init,
                    then,
                    otherwise,
                    ..
                } => {
                    if let Some(i) = init {
                        let init_block = hy_syntax::Block {
                            stmts: vec![(**i).clone()],
                            span: i.span(),
                        };
                        if !Self::block_returns_only_fresh_append(&init_block, saw_return) {
                            return false;
                        }
                    }
                    if !Self::block_returns_only_fresh_append(then, saw_return) {
                        return false;
                    }
                    if let Some(o) = otherwise {
                        let dummy_block = hy_syntax::Block {
                            stmts: vec![(**o).clone()],
                            span: o.span(),
                        };
                        if !Self::block_returns_only_fresh_append(&dummy_block, saw_return) {
                            return false;
                        }
                    }
                }
                hy_syntax::Stmt::For { kind, body, .. } => {
                    match kind {
                        hy_syntax::ForKind::Range { var, .. } if var == "append" => {
                            return false;
                        }
                        hy_syntax::ForKind::Iterate { vars, .. }
                            if vars.iter().any(|v| v == "append") =>
                        {
                            return false;
                        }
                        _ => {}
                    }
                    if !Self::block_returns_only_fresh_append(body, saw_return) {
                        return false;
                    }
                }
                hy_syntax::Stmt::Managed { body, .. } => {
                    if !Self::block_returns_only_fresh_append(body, saw_return) {
                        return false;
                    }
                }
                hy_syntax::Stmt::Block { block, .. } => {
                    if !Self::block_returns_only_fresh_append(block, saw_return) {
                        return false;
                    }
                }
                hy_syntax::Stmt::Match { arms, .. } => {
                    for arm in arms {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(b), ..
                        } = &arm.pattern
                        {
                            if b == "append" {
                                return false;
                            }
                        }
                        if !Self::block_returns_only_fresh_append(&arm.body, saw_return) {
                            return false;
                        }
                    }
                }
                hy_syntax::Stmt::Select { arms, .. } => {
                    for arm in arms {
                        if let hy_syntax::SelectArmKind::Recv { names, .. } = &arm.kind {
                            if names.iter().any(|n| n == "append") {
                                return false;
                            }
                        }
                        if !Self::block_returns_only_fresh_append(&arm.body, saw_return) {
                            return false;
                        }
                    }
                }
                _ => {}
            }
        }
        true
    }

    fn is_fresh_empty_slice_append(expr: &Expr) -> bool {
        let Expr::Call { callee, args, .. } = crate::model::unwrap_parens(expr) else {
            return false;
        };
        let Expr::Ident { name, .. } = crate::model::unwrap_parens(callee) else {
            return false;
        };
        if name != "append" || args.is_empty() {
            return false;
        }
        matches!(
            crate::model::unwrap_parens(&args[0].value),
            Expr::SliceLit { items, .. } if items.is_empty()
        )
    }
}

/// Does `block` contain any `continue` statement targeting the loop whose body
/// is `block` (i.e. not enclosed in an inner `for` loop)?
fn block_has_continue_for_loop(block: &hy_syntax::Block) -> bool {
    block.stmts.iter().any(stmt_has_continue_for_loop)
}

fn stmt_has_continue_for_loop(stmt: &hy_syntax::Stmt) -> bool {
    match stmt {
        hy_syntax::Stmt::Continue { .. } => true,
        hy_syntax::Stmt::If {
            then, otherwise, ..
        } => {
            block_has_continue_for_loop(then)
                || otherwise
                    .as_deref()
                    .map(stmt_has_continue_for_loop)
                    .unwrap_or(false)
        }
        hy_syntax::Stmt::Match { arms, .. } => {
            arms.iter().any(|a| block_has_continue_for_loop(&a.body))
        }
        hy_syntax::Stmt::Select { arms, .. } => {
            arms.iter().any(|a| block_has_continue_for_loop(&a.body))
        }
        hy_syntax::Stmt::Block { block, .. } | hy_syntax::Stmt::Managed { body: block, .. } => {
            block_has_continue_for_loop(block)
        }
        // A `continue` (or `continue loop`, LX14) inside an inner `for` targets
        // that inner loop, not the outer loop.
        hy_syntax::Stmt::For { .. } => false,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use crate::{check_file, Verdict};
    use hy_syntax::parse_file;

    fn check(src: &str) -> crate::CheckResult {
        let ast = parse_file("freeze_test.hy", src).expect("snippet parses");
        check_file(&ast)
    }

    #[test]
    fn loop_backedge_pops_trailing_block_scopes_for_shadowed_root_and_alias() {
        // Gap 1a: inner alias moved while outer root `m` is shadowed in a
        // trailing block at `loop_end` (before `ScopeEvent::Pop` at `loop_end + 1`).
        let r_shadow_root = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"worker\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {\n\
                 m[\"main\"] = i\n\
                 if true {\n\
                   m2 := m\n\
                   m := 1\n\
                   _ = m\n\
                   task worker(move m2, done)\n\
                 }\n\
               }\n\
               <-done\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_shadow_root.verdict,
            Verdict::Reject,
            "{:?}",
            r_shadow_root.diagnostics
        );
        assert!(r_shadow_root
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // Gap 1b: outer root `m` moved while outer alias `m2` is shadowed in a
        // trailing block at `loop_end`.
        let r_shadow_alias = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"worker\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               m2 := m\n\
               done := make(chan bool, 1)\n\
               for i in 0..2 {\n\
                 m2[\"main\"] = i\n\
                 if i == 0 {\n\
                   m2 := 1\n\
                   _ = m2\n\
                   task worker(move m, done)\n\
                 }\n\
               }\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_shadow_alias.verdict,
            Verdict::Reject,
            "{:?}",
            r_shadow_alias.diagnostics
        );
        assert!(r_shadow_alias
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn loop_backedge_checks_continue_before_trailing_break_or_return() {
        // Gap 2: a trailing `break` or `return` does not make the loop single-
        // iteration when a preceding branch can `continue` to the back-edge.
        let r_break = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"worker\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {\n\
                 m[\"main\"] = i\n\
                 task worker(move m, done)\n\
                 if i == 0 {\n\
                   continue\n\
                 }\n\
                 break\n\
               }\n\
               <-done\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_break.verdict,
            Verdict::Reject,
            "{:?}",
            r_break.diagnostics
        );
        assert!(r_break.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r_return = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"worker\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {\n\
                 task worker(move m, done)\n\
                 if i == 0 {\n\
                   continue\n\
                 }\n\
                 return\n\
               }\n\
             }",
        );
        assert_eq!(
            r_return.verdict,
            Verdict::Reject,
            "{:?}",
            r_return.diagnostics
        );
        assert!(r_return.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn loop_backedge_checks_for_iterate_collection_after_move() {
        // Gap 3: moving the collection `m` inside `for k, v in m` races the
        // ongoing runtime range iterator against the spawned task on the next
        // iteration step.
        let r_iter = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"worker\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"a\": 1, \"b\": 2}\n\
               done := make(chan bool, 1)\n\
               first := true\n\
               for k, v in m {\n\
                 _ = k\n\
                 _ = v\n\
                 if first {\n\
                   first = false\n\
                   task worker(move m, done)\n\
                 }\n\
               }\n\
               <-done\n\
             }",
        );
        assert_eq!(r_iter.verdict, Verdict::Reject, "{:?}", r_iter.diagnostics);
        assert!(r_iter.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }
}


