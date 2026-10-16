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
use std::collections::HashSet;

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
            if let Some((_, _, prior_mv)) = stmt_moves.iter().find(|(_, r, _)| r == &root) {
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
                    },
                ));
            }
        }
        // Also check if an earlier `non_via_move` read in the same statement
        // preceded a `via_move` transfer of the same binding/alias group.
        for u in uses {
            if !u.via_move {
                let root = resolve_alias(st, &u.name);
                if let Some((mv_name, _, mv_info)) = stmt_moves
                    .iter()
                    .find(|(_, r, mv)| r == &root && mv.span != u.span)
                {
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

    fn emit_use_after_move(&mut self, st: &FnState, u: &Use, mv: &MoveInfo) {
        // A may-share peer (#27) is an alias in every way that matters here,
        // even when the single-parent `aliases` chain does not name it (e.g.
        // a projection or composite-literal capture).
        let is_alias =
            st.aliases.contains_key(&u.name) || st.may_share.contains_key(&u.name);
        let orig = resolve_alias(st, &u.name);
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
                orig, mv.desc, self.path, span_str
            ));
            notes.push(
                "the transfer site is reported above so you can see where ownership left — not just the line that noticed".to_string(),
            );
            if is_alias {
                notes.push(format!(
                    "`{}` aliases `{}` — the consume freeze covers every live alias too (O13)",
                    u.name, orig
                ));
            }
        } else {
            notes.push(format!(
                "ownership of `{}` left this task at {} ({}:{})",
                orig, mv.desc, self.path, span_str
            ));
            if is_alias {
                notes.push(format!(
                    "`{}` aliases `{}` — the freeze covers every live alias, otherwise the alias binding would defeat it entirely (O5.2)",
                    u.name, orig
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
        let root = resolve_alias(st, name);
        let mut members: Vec<String> = vec![name.to_string(), root.clone()];
        for a in st.aliases.keys() {
            if resolve_alias(st, a) == root {
                members.push(a.clone());
            }
        }
        // #27: a move also freezes everything that may share the same mutable
        // GC storage — shallow struct/array copies, re-slices, field/index
        // projections, and composite literals that embedded a live binding.
        // The single-parent `aliases` chain above cannot express that graph,
        // so union in the may-share connected component of every member.
        let seeded = members.clone();
        for m in seeded {
            members.extend(st.share_group(&m));
        }
        members.sort();
        members.dedup();
        for m in &members {
            if let Some(b) = st.env_lookup_mut(m) {
                b.moved = Some(MoveInfo {
                    span,
                    kind,
                    desc: desc.clone(),
                });
            } else if let Some(cb) = self.consts.get(m).cloned() {
                if !cb.shape.is_sendable_by_type() {
                    let mut frozen = cb;
                    frozen.moved = Some(MoveInfo {
                        span,
                        kind,
                        desc: desc.clone(),
                    });
                    st.env.insert(m.clone(), frozen);
                }
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
            } else {
                self.check_o54_non_ident_move(st, op, mspan);
            }
        }
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
        let mut origin: Option<(String, bool)> = own.map(|o| (o, false));
        if origin.is_none() {
            let mut all_peers = st.share_group(name);
            if root != name {
                all_peers.extend(st.share_group(&root));
            }
            let mut peers: Vec<String> = all_peers.into_iter().collect();
            peers.sort();
            for peer in peers {
                let proot = resolve_alias(st, &peer);
                let po = self
                    .lookup_binding(st, &peer)
                    .and_then(|pb| pb.non_owned_origin.clone())
                    .or_else(|| {
                        self.lookup_binding(st, &proot)
                            .and_then(|rb| rb.non_owned_origin.clone())
                    });
                if let Some(po) = po {
                    origin = Some((po, true));
                    break;
                }
            }
        }
        origin
    }

    /// #194: true when `callee` is declared in this file and at least one of
    /// its `return` statements hands back one of its own parameters. The
    /// loop-backedge exemption below may only assume a FRESH result for
    /// callees that are not of this shape: `fn same(m map[string]int) ... {
    /// return m }` really does hand the caller's storage back, so a move
    /// through a local initialized by that call is loop-carried and must not
    /// be erased. The fresh-copy control (corpus 15's `chunk_of`, which
    /// returns `append([]string{}, …)`) keeps its exemption.
    fn callee_returns_own_param(&self, callee: &hy_syntax::Expr) -> bool {
        let Some(name) = crate::walk::expr_path(callee) else {
            return false;
        };
        let Some(f) = self.file.fns().find(|f| f.name == name) else {
            return false;
        };
        let Some(body) = &f.body else {
            return false;
        };
        let params: HashSet<&str> = f.params.iter().map(|p| p.name.as_str()).collect();
        crate::walk::flatten(body)
            .stmts
            .iter()
            .any(|(stmt, _)| match stmt {
                hy_syntax::Stmt::Return { values, .. } => values.iter().any(|v| {
                    matches!(
                        crate::model::unwrap_parens(v),
                        hy_syntax::Expr::Ident { name, .. } if params.contains(name.as_str())
                    )
                }),
                _ => false,
            })
    }

    /// O5.1 / O5.2 / O13 across loop iterations: when a loop body can repeat,
    /// any outer binding that was moved or consumed inside the loop and remains
    /// moved at the end of the body would be used or moved again on the next
    /// iteration. Walk the loop's statements along the back-edge with those
    /// loop-carried moves active; local definitions (`chunk := ...`, corpus 15)
    /// live in inner scopes and are discarded when `scopes` is truncated to
    /// `outer_depth`.
    pub(crate) fn check_loop_backedge(
        &mut self,
        st: &FnState,
        outer_depth: usize,
        loop_stmts: &[(&hy_syntax::Stmt, usize)],
        loop_body: &hy_syntax::Block,
        pre_moved: &HashSet<String>,
        reported_backedge: &mut HashSet<String>,
    ) {
        if matches!(
            loop_body.stmts.last(),
            Some(hy_syntax::Stmt::Break { .. } | hy_syntax::Stmt::Return { .. })
        ) {
            return;
        }

        let mut call_local_roots: HashSet<String> = HashSet::new();
        for (stmt, _) in loop_stmts.iter().skip(1) {
            if let hy_syntax::Stmt::Local(l) = stmt {
                for (lhs, rhs) in l.names.iter().zip(l.values.iter()) {
                    let is_inner = st.scopes[outer_depth..]
                        .iter()
                        .any(|s| s.contains_key(lhs.as_str()));
                    // #194: the exemption may only assume a FRESH call result.
                    // When the callee is declared in this file and returns one
                    // of its own parameters (`same(m) { return m }`), the alias
                    // the collector recorded is REAL, so the move through this
                    // local is loop-carried and must survive the reset below.
                    let returns_own_param = match crate::model::unwrap_parens(rhs) {
                        Expr::Call { callee, .. } => self.callee_returns_own_param(callee),
                        _ => false,
                    };
                    if is_inner
                        && matches!(crate::model::unwrap_parens(rhs), Expr::Call { .. })
                        && st.aliases.contains_key(lhs.as_str())
                        && !returns_own_param
                    {
                        call_local_roots.insert(resolve_alias(st, lhs));
                    }
                }
            }
        }

        let mut iter_st = st.clone();
        iter_st.scopes.truncate(outer_depth);
        let mut any_loop_carried = false;
        for (name, b) in iter_st
            .env
            .iter_mut()
            .chain(iter_st.scopes.iter_mut().flat_map(|s| s.iter_mut()))
        {
            if b.moved.is_some() {
                let root = resolve_alias(st, name);
                if pre_moved.contains(name)
                    || reported_backedge.contains(&root)
                    || call_local_roots.contains(&root)
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

        for (i, (stmt, _)) in loop_stmts.iter().enumerate() {
            let mut uses = Vec::new();
            if i == 0 {
                if let hy_syntax::Stmt::For {
                    kind: hy_syntax::ForKind::While { cond },
                    ..
                } = stmt
                {
                    crate::walk::expr_uses(cond, &mut uses);
                }
            } else {
                crate::walk::own_uses(stmt, &mut uses);
            }

            for u in &uses {
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
                for k in &clear_names {
                    if let Some(b) = iter_st.env_lookup_mut(k) {
                        b.moved = None;
                    }
                }
            }

            if i > 0 {
                if let hy_syntax::Stmt::Local(l) = stmt {
                    for name in &l.names {
                        iter_st.aliases.remove(name);
                        if let Some(b) = iter_st.env_lookup_mut(name) {
                            b.moved = None;
                        }
                    }
                }
            }
        }
    }
}

