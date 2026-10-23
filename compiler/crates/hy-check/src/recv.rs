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
                        self.bind_recv_match_arm(st, &mut arm_st, subject, &arm.pattern);
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

    fn resolve_recv_expr_shape(&self, st: &FnState, expr: &Expr) -> TypeShape {
        match self.shape_of_expr(st, expr) {
            TypeShape::Unknown => self
                .recv_chan_type_expr(st, expr)
                .map(|t| self.shape_of_type(&t))
                .unwrap_or(TypeShape::Unknown),
            s => s,
        }
    }

    fn recv_chan_type_expr(&self, st: &FnState, expr: &Expr) -> Option<hy_syntax::TypeExpr> {
        use hy_syntax::TypeExpr;
        match expr {
            Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. } => {
                self.recv_chan_type_expr(st, inner)
            }
            Expr::Try { operand, .. } => {
                let inner = self.recv_chan_type_expr(st, operand)?;
                match self.resolve_type_alias(&inner) {
                    TypeExpr::Option { elem, .. } => Some(*elem),
                    _ => Some(inner),
                }
            }
            Expr::Receive { chan, .. } => {
                let outer_ty = self.recv_chan_type_expr(st, chan)?;
                match self.resolve_type_alias(&outer_ty) {
                    TypeExpr::Chan { elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::Slice { base, span, .. } => {
                let base_ty = self.recv_chan_type_expr(st, base)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        Some(TypeExpr::Slice { elem, span: *span })
                    }
                    TypeExpr::Pointer { to, .. } => match self.resolve_type_alias(&to) {
                        TypeExpr::Array { elem, .. } => {
                            Some(TypeExpr::Slice { elem, span: *span })
                        }
                        _ => None,
                    },
                    _ => None,
                }
            }
            Expr::Index { base, .. } => {
                let base_ty = self.recv_chan_type_expr(st, base)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. }
                    | TypeExpr::Array { elem, .. }
                    | TypeExpr::Map { value: elem, .. } => Some(*elem),
                    TypeExpr::Pointer { to, .. } => match self.resolve_type_alias(&to) {
                        TypeExpr::Array { elem, .. } => Some(*elem),
                        _ => None,
                    },
                    _ => None,
                }
            }
            Expr::SliceLit {
                elem: None,
                items,
                span,
            } if expr.as_array_lit().is_none() => {
                let first_ty = self.recv_chan_type_expr(st, items.first()?)?;
                Some(TypeExpr::Slice {
                    elem: Box::new(first_ty),
                    span: *span,
                })
            }
            Expr::Call { callee, args, span } => {
                if let Expr::Ident { name, .. } = crate::model::unwrap_parens(callee) {
                    if self.lookup_binding(st, name).is_none() {
                        match name.as_str() {
                            "append" => {
                                return self.recv_chan_type_expr(st, &args.first()?.value);
                            }
                            "some" => {
                                let inner = self.recv_chan_type_expr(st, &args.first()?.value)?;
                                return Some(TypeExpr::Option {
                                    elem: Box::new(inner),
                                    span: *span,
                                });
                            }
                            "make" => {
                                let first = crate::model::unwrap_parens(&args.first()?.value);
                                if let Expr::Ident { name: tn, span: ts } = first {
                                    if self.type_aliases.contains_key(tn) {
                                        return Some(TypeExpr::Named {
                                            name: tn.clone(),
                                            span: *ts,
                                        });
                                    }
                                }
                                return self.recv_chan_type_expr(st, first);
                            }
                            _ => {}
                        }
                    }
                }
                self.ty_of_expr(st, expr)
            }
            _ => self.ty_of_expr(st, expr),
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
        match self.resolve_recv_expr_shape(st, chan) {
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
    fn bind_recv_match_arm(
        &mut self,
        eval_st: &FnState,
        arm_st: &mut FnState,
        subject: &Expr,
        pattern: &hy_syntax::MatchPattern,
    ) {
        let hy_syntax::MatchPattern::Path {
            segments,
            binding: Some(b),
        } = pattern
        else {
            return;
        };
        if segments == &["some"] {
            self.bind_match_arm(arm_st, b, subject, false);
        }
        if let Some(ty) = self.resolve_match_arm_payload_type(eval_st, subject, segments) {
            let shape = self.shape_of_type(&ty);
            if b != "_" {
                self.define_binding(arm_st, b, shape, Some(ty), false, false, false);
            }
        }
    }

    fn resolve_match_arm_payload_type(
        &self,
        st: &FnState,
        subject: &Expr,
        segments: &[String],
    ) -> Option<hy_syntax::TypeExpr> {
        let subj_ty = self.recv_chan_type_expr(st, subject)?;
        let resolved = self.resolve_type_alias(&subj_ty);
        if segments == &["some"] {
            if let hy_syntax::TypeExpr::Option { elem, .. } = resolved {
                return Some(*elem);
            }
        }
        let vname = segments.last()?.as_str();
        let hy_syntax::TypeExpr::Named { name, span } = &resolved else {
            return None;
        };
        let base = Self::base_named_type_str(name);
        if segments.len() == 2 && segments[0] != base {
            return None;
        }
        let type_args = split_type_args(name, *span);
        for d in &self.file.decls {
            if let hy_syntax::Decl::Enum(ed) = d {
                if ed.name == base {
                    let variant = ed.variants.iter().find(|v| v.name == vname)?;
                    let payload = variant.payload.as_ref()?;
                    let mut tps = Vec::new();
                    for p in ed.variants.iter().filter_map(|v| v.payload.as_ref()) {
                        self.collect_unresolved_type_names(p, &mut tps);
                    }
                    return Some(subst_type(payload, &tps, &type_args));
                }
            }
        }
        match (base.eq_ignore_ascii_case("result"), vname) {
            (true, "ok") => type_args.first().cloned(),
            (true, "err") => type_args.get(1).cloned(),
            _ => None,
        }
    }

    fn collect_unresolved_type_names(&self, ty: &hy_syntax::TypeExpr, out: &mut Vec<String>) {
        use hy_syntax::TypeExpr::*;
        match ty {
            Named { name, .. } => {
                let b = Self::base_named_type_str(name);
                let known = matches!(
                    b,
                    "int"
                        | "int8"
                        | "int16"
                        | "int32"
                        | "int64"
                        | "uint"
                        | "uint8"
                        | "uint16"
                        | "uint32"
                        | "uint64"
                        | "uintptr"
                        | "float32"
                        | "float64"
                        | "bool"
                        | "string"
                        | "byte"
                        | "rune"
                        | "any"
                        | "error"
                ) || self.structs.contains_key(b)
                    || self.enums.contains_key(b)
                    || self.type_aliases.contains_key(b);
                if !known && !out.iter().any(|p| p == b) {
                    out.push(b.to_string());
                }
            }
            Pointer { to, .. }
            | Slice { elem: to, .. }
            | Array { elem: to, .. }
            | Chan { elem: to, .. }
            | Option { elem: to, .. } => self.collect_unresolved_type_names(to, out),
            Map { key, value, .. } => {
                self.collect_unresolved_type_names(key, out);
                self.collect_unresolved_type_names(value, out);
            }
            Fn { .. } => {}
        }
    }
}

fn split_type_args(name: &str, span: Span) -> Vec<hy_syntax::TypeExpr> {
    let Some(inner) = name.split_once('[').and_then(|(_, r)| r.strip_suffix(']')) else {
        return Vec::new();
    };
    let pt = |s: &str| {
        if let Ok(f) = hy_syntax::parse_file("<type>", &format!("type a = {}", s.trim())) {
            if let Some(hy_syntax::Decl::TypeAlias(ta)) = f.decls.into_iter().next() {
                return ta.ty;
            }
        }
        hy_syntax::TypeExpr::Named {
            name: s.trim().to_string(),
            span,
        }
    };
    let (mut out, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    for (i, ch) in inner.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(pt(&inner[start..i]));
                start = i + 1;
            }
            _ => {}
        }
    }
    if start <= inner.len() && !inner[start..].trim().is_empty() {
        out.push(pt(&inner[start..]));
    }
    out
}

fn subst_type(
    ty: &hy_syntax::TypeExpr,
    params: &[String],
    args: &[hy_syntax::TypeExpr],
) -> hy_syntax::TypeExpr {
    use hy_syntax::TypeExpr::*;
    match ty {
        Named { name, .. } => params
            .iter()
            .position(|p| p == name)
            .and_then(|i| args.get(i).cloned())
            .unwrap_or_else(|| ty.clone()),
        Pointer { to, span } => Pointer {
            to: Box::new(subst_type(to, params, args)),
            span: *span,
        },
        Slice { elem, span } => Slice {
            elem: Box::new(subst_type(elem, params, args)),
            span: *span,
        },
        Array { len, elem, span } => Array {
            len: len.clone(),
            elem: Box::new(subst_type(elem, params, args)),
            span: *span,
        },
        Map { key, value, span } => Map {
            key: Box::new(subst_type(key, params, args)),
            value: Box::new(subst_type(value, params, args)),
            span: *span,
        },
        Chan { dir, elem, span } => Chan {
            dir: *dir,
            elem: Box::new(subst_type(elem, params, args)),
            span: *span,
        },
        Option { elem, span } => Option {
            elem: Box::new(subst_type(elem, params, args)),
            span: *span,
        },
        Fn { .. } => ty.clone(),
    }
}
