//! recv.rs — receive-operand channel-ness checks (HYB-E2008 / E3005).
//!
//! Issue #409: the send direction has been guarded since #77 —
//! `check_send_target` rejects a non-channel send target (HYB-E3007) and
//! fail-closes an unresolvable one (HYB-E3005). The receive direction had
//! no counterpart: `<-x` with `x` an int / map / non-channel call result
//! passed `hyb check` with 0 diagnostics and emitted Go that fails
//! `go build` (`cannot receive from non-channel`). This module mirrors the
//! send side one-for-one:
//!   * a receive whose operand resolves to a concrete non-channel shape is
//!     rejected (HYB-E2008);
//!   * an unresolvable operand stays fail-closed (HYB-E3005, O4.9) — the
//!     same treatment `check_send_target` gives an unknown send target.
//!
//! Wiring: every statement in the Tier-0 flat walk (lib.rs, after the
//! builtins pass) and every closure-body statement (closures.rs — FnLit
//! bodies sit outside the flat walk, the A5 perimeter) runs
//! `check_stmt_recv_operands`, which recurses the statement's own
//! expression trees (nested block statements arrive as their own flattened
//! entries) and classifies every `Expr::Receive` operand it finds, plus
//! `SelectArmKind::Recv` channel expressions.

use crate::model::{Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, InterpPart, PlaceKind, SelectArmKind, Stmt};

impl<'a> Checker<'a> {
    /// Statement entry: classify every receive operand in this statement's
    /// own expressions. Nested block bodies are NOT recursed — the Tier-0
    /// flat walk already lists those statements (same discipline as
    /// `arith.rs` / `builtins.rs`); the closure walk recurses explicitly.
    pub(crate) fn check_stmt_recv_operands(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) => {
                if let Some(ty) = &l.ty {
                    self.check_type_recv_operands(st, ty);
                }
                for v in &l.values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    self.check_place_recv_operands(st, t);
                }
                for v in values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_recv_operands(st, expr),
            Stmt::Send { chan, value, .. } => {
                // The send TARGET is `check_send_target`'s domain (E3007);
                // here we only look for receives NESTED in either side
                // (`ch <- <-done` and friends).
                self.check_expr_recv_operands(st, chan);
                self.check_expr_recv_operands(st, value);
            }
            // `if`/`for` init statements arrive as their own flattened
            // entries in Pass B (walk.rs walk_one), whereas closures.rs calls
            // `check_stmt_recv_operands` on `Stmt::If` before walking `init`.
            // Bind `init` locals into a temporary scope when checking `cond`
            // so `if ch := make(chan int, 1); (<-ch) > 0` resolves `ch` in
            // both passes without double-checking `init` itself.
            Stmt::If { init, cond, .. } => {
                if let Some(hy_syntax::Stmt::Local(l)) = init.as_deref() {
                    let mut tmp = st.clone();
                    tmp.scope_push();
                    let immutable = l.kind == hy_syntax::LocalKind::Const;
                    let is_let = l.kind == hy_syntax::LocalKind::Let;
                    if l.values.is_empty() {
                        let shape = l
                            .ty
                            .as_ref()
                            .map(|t| self.shape_of_type(t))
                            .unwrap_or(TypeShape::Unknown);
                        for name in &l.names {
                            self.define_binding(
                                &mut tmp,
                                name,
                                shape.clone(),
                                l.ty.clone(),
                                immutable,
                                false,
                                is_let,
                            );
                        }
                    } else if l.names.len() == l.values.len() {
                        // Parallel multi-name init (PR-665 composed with
                        // main's T22 is_let): RHS expressions are evaluated
                        // against the PRE-statement scope `st` (Go parallel
                        // assignment semantics), not the progressively
                        // defined `tmp`, so `if x, y := 1, x; ...` sees the
                        // outer `x` before the new `x` is bound.
                        let bindings: Vec<_> = l
                            .names
                            .iter()
                            .zip(l.values.iter())
                            .map(|(name, val)| {
                                let inferred = self.shape_of_expr(st, val);
                                let shape = match (&l.ty, inferred) {
                                    (Some(t), TypeShape::Unknown) => self.shape_of_type(t),
                                    (_, s) => s,
                                };
                                let ty = l.ty.clone().or_else(|| self.ty_of_expr(st, val));
                                (name, shape, ty)
                            })
                            .collect();
                        for (name, shape, ty) in bindings {
                            self.define_binding(&mut tmp, name, shape, ty, immutable, false, is_let);
                        }
                    } else if l.values.len() == 1 && l.names.len() > 1 {
                        let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
                        let tys = self.multi_value_tys(st, &l.values[0], l.names.len());
                        for ((name, shape), ty) in
                            l.names.iter().zip(shapes.into_iter()).zip(tys.into_iter())
                        {
                            self.define_binding(
                                &mut tmp,
                                name,
                                shape,
                                l.ty.clone().or(ty),
                                immutable,
                                false,
                                is_let,
                            );
                        }
                    }
                    self.check_expr_recv_operands(&tmp, cond);
                } else {
                    self.check_expr_recv_operands(st, cond);
                }
            }
            Stmt::For { kind, .. } => match kind {
                hy_syntax::ForKind::While { cond } => self.check_expr_recv_operands(st, cond),
                hy_syntax::ForKind::Range { lo, hi, .. } => {
                    self.check_expr_recv_operands(st, lo);
                    self.check_expr_recv_operands(st, hi);
                }
                hy_syntax::ForKind::Iterate { expr, .. } => self.check_expr_recv_operands(st, expr),
                hy_syntax::ForKind::Infinite => {}
            },
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_recv_operands(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_recv_operands(st, callee);
                for a in args {
                    self.check_expr_recv_operands(st, &a.value);
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_recv_operands(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        let mut arm_st = st.clone();
                        arm_st.scope_push();
                        if let hy_syntax::MatchPattern::Path {
                            segments,
                            binding: Some(b),
                        } = &arm.pattern
                        {
                            if segments == &["some"] {
                                self.bind_match_arm(&mut arm_st, b, subject, false);
                                if let Some(ty) = self.ty_of_expr(st, subject) {
                                    if let hy_syntax::TypeExpr::Option { elem, .. } =
                                        self.resolve_type_alias(&ty)
                                    {
                                        if let Some(binding) = arm_st.env_lookup_mut(b) {
                                            binding.ty = Some(*elem);
                                        }
                                    }
                                }
                            }
                        }
                        self.check_expr_recv_operands(&arm_st, g);
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_recv_operands(st, call),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_recv_operands(st, chan);
                            self.check_expr_recv_operands(st, value);
                        }
                        // The receive ARM's channel is a receive operand
                        // itself (`select { case v := <-x: }` with `x` an
                        // int used to pass with 0 diagnostics).
                        SelectArmKind::Recv { chan, .. } => {
                            let span = chan.span();
                            self.check_recv_operand(st, chan, span);
                            self.check_expr_recv_operands(st, chan);
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

    fn check_type_recv_operands(&mut self, st: &FnState, ty: &hy_syntax::TypeExpr) {
        match ty {
            hy_syntax::TypeExpr::Named { .. } => {}
            hy_syntax::TypeExpr::Pointer { to, .. } => self.check_type_recv_operands(st, to),
            hy_syntax::TypeExpr::Slice { elem, .. }
            | hy_syntax::TypeExpr::Chan { elem, .. }
            | hy_syntax::TypeExpr::Option { elem, .. } => self.check_type_recv_operands(st, elem),
            hy_syntax::TypeExpr::Array { len, elem, .. } => {
                if let Some(len) = len {
                    self.check_expr_recv_operands(st, len);
                }
                self.check_type_recv_operands(st, elem);
            }
            hy_syntax::TypeExpr::Map { key, value, .. } => {
                self.check_type_recv_operands(st, key);
                self.check_type_recv_operands(st, value);
            }
            hy_syntax::TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.check_type_recv_operands(st, &p.ty);
                }
                for r in results {
                    self.check_type_recv_operands(st, r);
                }
            }
        }
    }

    fn check_place_recv_operands(&mut self, st: &FnState, place: &hy_syntax::Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_recv_operands(st, base);
                self.check_expr_recv_operands(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_recv_operands(st, base);
            }
        }
    }

    /// Recurse an expression tree, classifying every `Expr::Receive`
    /// operand found at any depth. FnLit bodies are the A5 perimeter —
    /// not walked here (the closure pass enters them).
    fn check_expr_recv_operands(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Receive { chan, span } => {
                self.check_recv_operand(st, chan, *span);
                self.check_expr_recv_operands(st, chan);
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_recv_operands(st, callee);
                for a in args {
                    self.check_expr_recv_operands(st, &a.value);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_recv_operands(st, lhs);
                self.check_expr_recv_operands(st, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.check_expr_recv_operands(st, operand),
            Expr::Index { base, index, .. } => {
                self.check_expr_recv_operands(st, base);
                self.check_expr_recv_operands(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_recv_operands(st, base);
                if let Some(lo) = lo {
                    self.check_expr_recv_operands(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_recv_operands(st, hi);
                }
            }
            Expr::Field { base, .. } => self.check_expr_recv_operands(st, base),
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_recv_operands(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_recv_operands(st, it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_recv_operands(st, k);
                    self.check_expr_recv_operands(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_recv_operands(st, e);
                    }
                }
            }
            Expr::FnLit { .. } | Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    /// Project the resolved TypeExpr of an expression the `ty_of_expr` cache
    /// does not cover — composite channel/slice operands, nested receives,
    /// `append(...)` results (see `recv_chan_type_expr` for the arm list).
    /// Reused as the define-binding `ty` fallback in `process_local` so a
    /// local bound from a composite (`sub := append(chs, ch)`) carries a
    /// projected type; before this, such a binding had `ty: None`, nothing
    /// to project at a later `<-sub[0]` in a type position (where the
    /// E3008 direction walk does not run), and the send-only misuse passed
    /// with 0 diagnostics — the "known remaining sibling" NOTE in
    /// `tests/checker-corpus` / `recv_composite_channel_operands.rs`.
    pub(crate) fn composite_type_expr(&self, st: &FnState, expr: &Expr) -> Option<hy_syntax::TypeExpr> {
        self.recv_chan_type_expr(st, expr)
    }

    fn recv_chan_type_expr(&self, st: &FnState, expr: &Expr) -> Option<hy_syntax::TypeExpr> {
        use hy_syntax::TypeExpr;
        if let Some(ty) = self.ty_of_expr(st, expr) {
            return Some(ty);
        }
        match expr {
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } => self.recv_chan_type_expr(st, inner),
            Expr::Receive { chan, .. } => {
                let outer_ty = self.recv_chan_type_expr(st, chan)?;
                match self.resolve_type_alias(&outer_ty) {
                    TypeExpr::Chan { elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            // #1347 part 1: composite channel expressions. The type caches
            // behind `ty_of_expr` do not project through indexing, slicing or
            // builtin calls, so `<-append(chs, ch)[0]` and `<-(<-outer)[0]`
            // resolved to `None` here while `shape_of_expr` still reported
            // `TypeShape::Chan(_)`. `check_recv_operand` then returned `true`
            // with no diagnostic in the positions where the E3008 direction
            // walk does not run (type positions such as array lengths), and
            // the accepted program emitted Go that fails `go build` with
            // `invalid operation: cannot receive from send-only channel`.
            // These arms mirror `sendability::dir_ty_of_expr`'s projections.
            Expr::Index { base, .. } => {
                let base_ty = self.recv_chan_type_expr(st, base)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. }
                    | TypeExpr::Array { elem, .. }
                    | TypeExpr::Map { value: elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::Slice { base, span, .. } => {
                let base_ty = self.recv_chan_type_expr(st, base)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        Some(TypeExpr::Slice { elem, span: *span })
                    }
                    _ => None,
                }
            }
            // A local whose `ty_of_expr` entry is missing (the initializer was
            // a call the type cache does not project) still carries the
            // written type on its binding.
            Expr::Ident { name, .. } => st.env_lookup(name).and_then(|b| b.ty.clone()),
            Expr::Call { callee, args, span } => {
                // `append(s, …)` returns `s`'s own slice/array type; unwrap
                // parens so `(append(s, v))[0]` resolves too.
                let mut head = callee.as_ref();
                while let Expr::Paren { inner, .. } = head {
                    head = inner.as_ref();
                }
                if let Expr::Ident { name, .. } = head {
                    if name == "append" && !self.fns.contains_key("append") {
                        let first = args.first()?;
                        return self.recv_chan_type_expr(st, &first.value);
                    }
                }
                let _ = span;
                None
            }
            _ => None,
        }
    }

    /// Classify a receive's SOURCE — the mirror of `check_send_target`
    /// (issue #409): a receive is only well-typed when the channel
    /// expression resolves to a channel shape. A concrete non-channel is
    /// HYB-E2008; an unresolvable shape is fail-closed HYB-E3005 (O4.9),
    /// exactly as the send side treats an unknown target. Like the send
    /// side, an ill-typed source is not a sendability doubt — `managed`
    /// does not suppress it.
    pub(crate) fn check_recv_operand(&mut self, st: &FnState, chan: &Expr, span: Span) -> bool {
        match self.shape_of_expr(st, chan) {
            TypeShape::Chan(_) => {
                if let Some(ty) = self.recv_chan_type_expr(st, chan) {
                    let resolved = self.resolve_type_alias(&ty);
                    if matches!(
                        resolved,
                        hy_syntax::TypeExpr::Chan {
                            dir: hy_syntax::ChanDir::Send,
                            ..
                        }
                    ) {
                        // HYB-E3008 (chan-direction-mismatch) owns directed-channel
                        // misuse on main; this generic arm only reports when E3008
                        // has not already diagnosed the same site, so a defect is
                        // reported once by its precise code.
                        if !self
                            .diags
                            .iter()
                            .any(|d| d.code == "HYB-E3008" && d.span == span)
                        {
                            let msg = format!(
                                "cannot receive from `{}` (`{}`) — channel is send-only (HYB-E2008)",
                                chan.render(),
                                resolved.render()
                            );
                            let notes = vec![
                                "why: Go rejects `<-ch` on a `chan<- T` send-only channel (`invalid operation: cannot receive from send-only channel`), so an accepted Hybrid program would emit unbuildable Go".to_string(),
                                "fix: declare the parameter or binding as a bidirectional (`chan T`) or receive-only (`<-chan T`) channel".to_string(),
                            ];
                            self.diags.push(
                                Diagnostic::error("HYB-E2008", self.path.clone(), span, msg)
                                    .with_notes(notes),
                            );
                        }
                        return false;
                    }
                }
                true
            }
            TypeShape::Unknown => {
                let msg = format!(
                    "cannot prove `{}` is a channel — a receive source must resolve to `chan T` (O4.9)",
                    chan.render()
                );
                let notes = vec![
                    "why: the checker cannot classify the receive source's type, and a receive from a non-channel is not valid Go (issue #409)".to_string(),
                    "fix: bind the channel to a local (or parameter) with a visible `chan T` type before receiving, so the checker can resolve it".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3005", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
            other => {
                let msg = format!(
                    "cannot receive from `{}` ({}) — a receive source must be a channel (O4)",
                    chan.render(),
                    other.render()
                );
                let notes = vec![
                    "why: `<-` on a non-channel value is not valid Go; the accepted subset only emits buildable Go (issue #409 — the send side of this rule is HYB-E3007, issue #77)".to_string(),
                    "fix: receive from a `chan T` binding (`ch := make(chan T, n)` or a `chan T` parameter); a statement STARTING with `<-expr` is a receive — check the operand's type".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2008", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
        }
    }
}
