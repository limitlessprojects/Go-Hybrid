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

/// Follow an alias chain to its root (bounded: alias cycles are a parser
/// bug, not a checker concern, but never loop forever on one).
pub(crate) fn resolve_alias(st: &FnState, name: &str) -> String {
    let mut cur = name.to_string();
    for _ in 0..16 {
        match st.aliases.get(&cur) {
            Some(next) => cur = next.clone(),
            None => break,
        }
    }
    cur
}

impl<'a> Checker<'a> {
    // -----------------------------------------------------------------------
    // Use-after-move / use-after-consume detection
    // -----------------------------------------------------------------------

    /// Check one statement's uses against the frozen state. One diagnostic
    /// per (statement, binding): the first use site is reported.
    pub(crate) fn check_uses(&mut self, st: &FnState, uses: &[Use]) {
        let mut reported: HashSet<String> = HashSet::new();
        for u in uses {
            if !reported.insert(u.name.clone()) {
                continue;
            }
            let moved = match self.lookup_binding(st, &u.name) {
                Some(b) => b.moved.clone(),
                None => continue,
            };
            let Some(mv) = moved else {
                continue; // via_move uses of live bindings are transfers, exempt
            };
            self.emit_use_after_move(st, u, &mv);
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
    /// or a range variable over a non-channel collection remains aliased in
    /// the caller / enclosing scope / collection, so moving it is rejected
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
        if b.shape.is_sendable_by_type() || b.immutable || b.managed_taint {
            return;
        }
        let root = resolve_alias(st, name);
        let mut origin = b.non_owned_origin.clone().or_else(|| {
            self.lookup_binding(st, &root)
                .and_then(|rb| rb.non_owned_origin.clone())
        });
        if origin.is_none() {
            // #75: moving a may-share peer of a non-owned binding (e.g. a shallow
            // struct or array copy of a parameter) escapes the O5.1 non-owned check
            // because the caller retains live access to the underlying storage.
            let mut all_peers = st.share_group(name);
            if root != name {
                all_peers.extend(st.share_group(&root));
            }
            let mut peers: Vec<String> = all_peers.into_iter().collect();
            peers.sort();
            for peer in peers {
                let peer_root = resolve_alias(st, &peer);
                origin = self
                    .lookup_binding(st, &peer)
                    .and_then(|pb| pb.non_owned_origin.clone())
                    .or_else(|| {
                        self.lookup_binding(st, &peer_root)
                            .and_then(|prb| prb.non_owned_origin.clone())
                    });
                if origin.is_some() {
                    break;
                }
            }
        }
        let Some(origin) = origin else {
            return;
        };
        let ty_str = b
            .ty
            .as_ref()
            .map(|t| t.render())
            .unwrap_or_else(|| b.shape.render());
        let msg = format!(
            "cannot move `{}` ({}) — `move` requires an exclusively owned local binding, not {} (O5.1 / O5.4)",
            name, ty_str, origin
        );
        let notes = vec![
            format!(
                "why: {} still aliases the underlying storage outside this move site, so freezing `{}` cannot prevent concurrent access (O5.2 / O5.4)",
                origin, name
            ),
            format!(
                "fix: copy `{}` into a fresh local before moving (`fresh := ...; move fresh`), move the caller's owned binding at the spawn/send site directly, or wrap a genuinely synchronized share in `managed(\"reason\", \"category\") {{ ... }}`",
                name
            ),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3004", self.path.clone(), span, msg).with_notes(notes),
        );
    }
}

