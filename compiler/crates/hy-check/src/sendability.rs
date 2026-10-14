//! sendability.rs — rule A1c (O4 classes) at spawn/send check-sites, plus
//! the O6 `managed` suppression and lint (HYB-E8001), the O4.9 FAIL-CLOSED
//! boundary (HYB-E3005, rule FC — human directive 2026-10-13), the O5.4
//! move-form check (HYB-E3004), the spawn-callee proof (O4.0: the callee
//! must resolve to a declared top-level `fn` — closure callees share their
//! captures with the new task, unknown callees are fail-closed), and the
//! N1 nil rule at boundaries and known-type sites (HYB-E4001).
//!
//! Outcome per value (task brief, VERDICTS.md sketches):
//!   * OK classes: primitives; string/const/immutable; chan-typed values;
//!     `move x`; fresh literals whose reference-typed parts are all sendable
//!     (O4.6 deep sendability); atomic values and `&atomic_binding`;
//!     managed-tainted values (class 6).
//!   * REJECT: a named mutable GC binding (map/slice/struct/pointer-to-
//!     mutable), `&mutable_struct_binding`, borrow-handles, closures.
//!   * UNPROVEN (fail-closed, O4.9): a value whose type shape did not
//!     resolve, or a name that did not resolve — silently passing was the
//!     Tier-0 default until the human owner ruled it must not. The escape
//!     hatch is explicit and auditable: `managed` (HYB-E8001, counted by
//!     --report-managed).

use crate::model::{as_addr_of, unwrap_parens, Binding, Checker, FnState, MoveKind, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Expr, LitKind};

/// The spawn-callee proof (O4.0): what the checker concluded about the
/// expression a `task` statement spawns.
pub(crate) enum CalleeVerdict {
    /// A declared top-level `fn` — a static call; nothing but the arguments
    /// crosses into the new task.
    Proven,
    /// A closure (a literal, or a closure-typed binding): spawning it
    /// shares its captured environment with the new task (A5).
    Closure(RejectInfo),
    /// Everything else (an unresolvable name, a method value, a computed
    /// callee) — fail-closed unproven (O4.9).
    Unproven(String),
}

/// The classification of one value at a check-site.
pub(crate) enum Outcome {
    /// Sendable via classes 1-5 (or already-reported use-after-move).
    Ok,
    /// Resolved via class 6 (managed taint / suppression) — sendable.
    /// `risky` marks values that are NOT also safe-by-type (mutable GC
    /// state): only those carry the audit lint and the class-6 site count —
    /// a tainted channel or primitive crossing a site is not the trust
    /// surface an auditor needs named (O6.3: the lint carries WHAT is
    /// shared and WHY it is believed safe).
    Managed { desc: String, risky: bool },
    /// Not sendable (A1c) — the checker SAW the type and it is mutable GC
    /// state (or a handle / closure).
    Rejected(RejectInfo),
    /// FAIL-CLOSED (O4.9): the checker could NOT resolve the type or the
    /// name, so it cannot prove the value safe. Never silently passes;
    /// escapes only through an explicit `managed` block (HYB-E3005 / FC).
    Unproven(UnprovenInfo),
}

pub(crate) struct UnprovenInfo {
    /// The value, as written ("blob", "&cfg", "results <- mystery()").
    pub value: String,
    /// Why the checker could not prove it ("its type did not resolve to a
    /// sendability class" / "the name did not resolve in this scope").
    pub reason: String,
}

pub(crate) struct RejectInfo {
    /// The value, as written ("counts", "&stats", "shared_map").
    pub value: String,
    /// The value's type, for the message ("map[string]int", "*pool_stats").
    pub ty: String,
    pub notes: Vec<String>,
}

impl<'a> Checker<'a> {
    // -----------------------------------------------------------------------
    // Classification
    // -----------------------------------------------------------------------

    pub(crate) fn classify_value(&self, st: &FnState, expr: &Expr) -> Outcome {
        // `move x` — class 3; the transfer marking + O5.4 validation happens at the site.
        if matches!(unwrap_parens(expr), Expr::Move { .. }) {
            return Outcome::Ok;
        }
        // literals and interpolated strings
        if matches!(expr, Expr::Lit { .. } | Expr::Interp { .. }) {
            return Outcome::Ok;
        }
        // closures capture their environment — not sendable in Tier 0
        if matches!(expr, Expr::FnLit { .. }) {
            return Outcome::Rejected(RejectInfo {
                value: expr.render(),
                ty: "closure".to_string(),
                notes: vec![
                    "closures capture their environment; they are not sendable at spawn/send sites in Tier 0 (closure escapes are the A5 perimeter)".to_string(),
                ],
            });
        }
        // `&binding` — the pointer shape
        if let Some(operand) = as_addr_of(expr) {
            return self.classify_addr_of(st, expr, operand);
        }
        match expr {
            Expr::Ident { name, .. } => self.classify_ident(st, name),
            Expr::StructLit { fields, .. } => {
                self.classify_composite(st, expr, fields.iter().map(|f| &f.value))
            }
            Expr::MapLit { entries, .. } => {
                let mut parts = Vec::new();
                for (k, v) in entries {
                    parts.push(k);
                    parts.push(v);
                }
                self.classify_composite(st, expr, parts.into_iter())
            }
            Expr::SliceLit { items, .. } => self.classify_composite(st, expr, items.iter()),
            // field/index/slice reads: classify the read's shape, but honor
            // immutable roots (views of const values stay immutable, OQ5)
            Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
                if let Some(root) = expr.root_ident() {
                    if let Some(b) = self.lookup_binding(st, root) {
                        if b.moved.is_some() {
                            return Outcome::Ok; // use-after-move already reported
                        }
                        if b.managed_taint {
                            return Outcome::Managed {
                                desc: self.managed_desc(root, b),
                                risky: !b.shape.is_sendable_by_type(),
                            };
                        }
                        if b.immutable {
                            return Outcome::Ok;
                        }
                    }
                }
                let shape = self.shape_of_expr(st, expr);
                self.outcome_for_shape(&shape, &expr.render())
            }
            // everything else (calls, receives, arithmetic, try, paren):
            // classify the result shape
            _ => {
                let shape = self.shape_of_expr(st, expr);
                self.outcome_for_shape(&shape, &expr.render())
            }
        }
    }

    fn classify_ident(&self, st: &FnState, name: &str) -> Outcome {
        let Some(b) = self.lookup_binding(st, name) else {
            // FAIL-CLOSED (O4.9): a name that does not resolve cannot be
            // proven safe — it used to pass silently; now it is HYB-E3005
            // unless the site is `managed`.
            return Outcome::Unproven(UnprovenInfo {
                value: name.to_string(),
                reason: "the name did not resolve in this scope".to_string(),
            });
        };
        if b.moved.is_some() {
            // already reported at the use site — do not double-report A1c
            return Outcome::Ok;
        }
        if b.managed_taint {
            return Outcome::Managed {
                desc: self.managed_desc(name, b),
                risky: !b.shape.is_sendable_by_type(),
            };
        }
        if b.immutable {
            return Outcome::Ok; // class 4 (let/const/const-decl)
        }
        if matches!(b.shape, TypeShape::Unknown) {
            // FAIL-CLOSED (O4.9): a binding whose type the checker could
            // not resolve cannot silently cross a boundary.
            return Outcome::Unproven(UnprovenInfo {
                value: name.to_string(),
                reason: unproven_atomic_reason(b).unwrap_or_else(|| {
                    "its type did not resolve to a sendability class (no ascription and the initializer's type was unresolvable)".to_string()
                }),
            });
        }
        if b.shape.is_sendable_by_type() {
            return Outcome::Ok; // classes 1/2
        }
        let ty = self.binding_ty_str(b);
        if b.shape.is_handle() {
            return Outcome::Rejected(RejectInfo {
                value: name.to_string(),
                ty,
                notes: vec![format!(
                    "`{}` is a boundary handle — handles cross tasks only via an owned `move` (O11/O13)",
                    name
                )],
            });
        }
        let mut notes = vec![format!(
            "this {} is shared across tasks: mutable shared state must cross a boundary the checker can see (Tier-0 rule O4)",
            b.shape.noun()
        )];
        if matches!(b.shape, TypeShape::Map(_, _)) {
            notes.push(
                "as written this also panics in Go — concurrent map writes are a runtime fatal; the checker tells you earlier".to_string(),
            );
        }
        Outcome::Rejected(RejectInfo {
            value: name.to_string(),
            ty,
            notes,
        })
    }

    fn classify_addr_of(&self, st: &FnState, expr: &Expr, operand: &Expr) -> Outcome {
        let Some(root) = operand.root_ident() else {
            // `&m[k]`-shaped or otherwise unrooted: classify the pointee shape
            let shape = self.shape_of_expr(st, operand);
            return self.outcome_for_shape(&shape, &expr.render());
        };
        let Some(b) = self.lookup_binding(st, root) else {
            // FAIL-CLOSED (O4.9): lending through a name that does not
            // resolve — cannot prove what is behind the `&`.
            return Outcome::Unproven(UnprovenInfo {
                value: expr.render(),
                reason: format!("the name `{root}` did not resolve in this scope"),
            });
        };
        if b.moved.is_some() {
            return Outcome::Ok;
        }
        if b.managed_taint {
            return Outcome::Managed {
                desc: self.managed_desc(root, b),
                risky: !b.shape.is_sendable_by_type(),
            };
        }
        if b.immutable {
            return Outcome::Ok;
        }
        if matches!(b.shape, TypeShape::Unknown) {
            // FAIL-CLOSED (O4.9): a borrow of an unproven value.
            return Outcome::Unproven(UnprovenInfo {
                value: expr.render(),
                reason: unproven_atomic_reason(b).unwrap_or_else(|| {
                    format!("the pointee `{root}` has an unresolvable type")
                }),
            });
        }
        match &b.shape {
            // `&hits` where hits is atomic-typed — the mediated route (OQ6)
            TypeShape::Atomic => Outcome::Ok,
            // `&ch` — channels are share-safe by type
            TypeShape::Chan(_) => Outcome::Ok,
            TypeShape::Struct(Some(sname)) => {
                let contested = self.struct_has_mutex_field(sname);
                let mut notes = vec![format!(
                    "`{}` is a mutable struct shared as a live borrow — mutation through it from several tasks is a data race",
                    root
                )];
                if contested {
                    notes.push(
                        "this struct is guarded by sync.mutex — race-free idiomatic Go, but Tier 0 does not credit locks at spawn sites (QUESTIONS.md Q1, reading (c))".to_string(),
                    );
                    notes.push(
                        "an \"externally synchronized\" amendment (checker-verified exclusive access behind a lock) is under discussion for a later tier — this rejection is expected to be revisited; it is tracked, not silent (OQ2)".to_string(),
                    );
                }
                Outcome::Rejected(RejectInfo {
                    value: expr.render(),
                    ty: format!("*{}", self.binding_ty_str(b)),
                    notes,
                })
            }
            _ => {
                Outcome::Rejected(RejectInfo {
                    value: expr.render(),
                    ty: format!("*{}", self.binding_ty_str(b)),
                    notes: vec![format!(
                        "`&{}` lends the whole mutable {} — a live borrow of mutable shared state, which is not one of the sendable classes (O4)",
                        root,
                        b.shape.noun()
                    )],
                })
            }
        }
    }

    /// O4.6 deep sendability: a fresh literal is sendable iff every
    /// reference-typed part is sendable; the first bad part is named.
    fn classify_composite<'e>(
        &self,
        st: &FnState,
        expr: &Expr,
        parts: impl Iterator<Item = &'e Expr>,
    ) -> Outcome {
        let mut managed: Option<(String, bool)> = None;
        for part in parts {
            match self.classify_value(st, part) {
                Outcome::Ok => {}
                Outcome::Managed { desc, risky } => {
                    // O6.2: taint propagates through composites; a managed
                    // mutable part makes the composite the trust surface.
                    let r = managed.map(|(_, r)| r).unwrap_or(false) || risky;
                    managed = Some((desc, r));
                }
                Outcome::Rejected(mut info) => {
                    info.notes.push(format!(
                        "`{}` is a live mutable value referenced by the fresh literal `{}` — a fresh literal is sendable only when every reference-typed part is (O4.6 deep sendability)",
                        info.value,
                        expr.render()
                    ));
                    return Outcome::Rejected(info);
                }
                Outcome::Unproven(mut info) => {
                    // fail-closed composites: an unproven part makes the
                    // whole literal unproven (O4.6 + O4.9)
                    info.reason = format!(
                        "the literal `{}` carries this unproven part (O4.6 deep sendability): {}",
                        expr.render(),
                        info.reason
                    );
                    info.value = expr.render();
                    return Outcome::Unproven(info);
                }
            }
        }
        match managed {
            // a composite holding a managed part is managed (O6.2 taint
            // propagates through composites)
            Some((desc, risky)) => Outcome::Managed { desc, risky },
            None => Outcome::Ok,
        }
    }

    fn outcome_for_shape(&self, shape: &TypeShape, value: &str) -> Outcome {
        if matches!(shape, TypeShape::Unknown) {
            // FAIL-CLOSED (O4.9): an expression whose result type did not
            // resolve (a foreign call not in the descriptor stubs, an index
            // into an unresolvable container, ...) cannot silently cross.
            return Outcome::Unproven(UnprovenInfo {
                value: value.to_string(),
                reason: "the expression's result type did not resolve to a sendability class"
                    .to_string(),
            });
        }
        if shape.is_sendable_by_type() {
            return Outcome::Ok;
        }
        let ty = shape.render();
        if shape.is_handle() {
            return Outcome::Rejected(RejectInfo {
                value: value.to_string(),
                ty: ty.clone(),
                notes: vec![format!(
                    "`{}` is a boundary handle — handles cross tasks only via an owned `move` (O11/O13)",
                    value
                )],
            });
        }
        Outcome::Rejected(RejectInfo {
            value: value.to_string(),
            ty: ty.clone(),
            notes: vec![format!(
                "this {} is shared across tasks: mutable shared state must cross a boundary the checker can see (Tier-0 rule O4)",
                shape.noun()
            )],
        })
    }

    fn managed_desc(&self, name: &str, b: &Binding) -> String {
        format!("`{}` ({})", name, self.binding_ty_str(b))
    }

    // -----------------------------------------------------------------------
    // The spawn-callee proof (O4.0) + the N1 nil rule (HYB-E4001)
    // -----------------------------------------------------------------------

    /// Is this expression the bare `nil` literal (possibly parenthesized)?
    pub(crate) fn is_nil_literal(e: &Expr) -> bool {
        matches!(
            unwrap_parens(e),
            Expr::Lit {
                kind: LitKind::Nil,
                ..
            }
        )
    }

    /// N1: which shapes have a `nil`? Only the foreign-Go perimeter (N2 —
    /// `ForeignOpaque`; `error` itself resolves to Unknown today, so a nil
    /// whose target is Unknown is UNPROVEN, never silently ok).
    pub(crate) fn is_nilable_shape(t: &TypeShape) -> bool {
        matches!(t, TypeShape::ForeignOpaque(_))
    }

    /// The spawn-callee proof (O4.0): a spawn is only proven when the callee
    /// resolves to a declared top-level `fn` — a static call where nothing
    /// but the arguments crosses. A closure callee shares its captured
    /// environment with the new task (A5 reject); an unresolvable callee is
    /// fail-closed unproven (O4.9). Local bindings shadow the fn table.
    fn spawn_callee_verdict(&self, st: &FnState, callee: &Expr) -> CalleeVerdict {
        let closure_reject = |value: String| {
            CalleeVerdict::Closure(RejectInfo {
                value,
                ty: "closure".to_string(),
                notes: vec![
                    "the spawned callee is a closure: closures capture their environment, so spawning one shares every capture with the new task (A5) — spawn a declared top-level `fn` instead".to_string(),
                    "if the closure is the design (an actor loop, a worker body), wrap the site in `managed(\"why this bypass is safe\", \"actor\") { ... }` — the explicit, auditable hatch (HYB-E8001, counted by --report-managed)".to_string(),
                ],
            })
        };
        match unwrap_parens(callee) {
            Expr::FnLit { .. } => closure_reject(callee.render()),
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if matches!(b.shape, TypeShape::Closure) {
                        return closure_reject(name.clone());
                    }
                    return CalleeVerdict::Unproven(format!(
                        "the spawn callee `{}` resolved to a local binding that is not a declared top-level `fn` — the spawn-callee proof (O4.0) only covers declared fns",
                        name
                    ));
                }
                if self.fns.contains_key(name) {
                    return CalleeVerdict::Proven;
                }
                CalleeVerdict::Unproven(format!(
                    "the spawn callee `{}` did not resolve to a declared top-level `fn` — the checker cannot prove what would run in the new task",
                    name
                ))
            }
            _ => CalleeVerdict::Unproven(format!(
                "the checker can only prove spawns of declared top-level `fn`s — `{}` is a method value or a computed callee, which the spawn-callee proof (O4.0) cannot verify",
                callee.render()
            )),
        }
    }

    /// The spawn callee's parameter shapes when the callee resolves to a
    /// declared top-level fn with matching arity (drives the N1 nil checks
    /// on spawn arguments). None when the callee does not resolve statically.
    pub(crate) fn spawn_param_shapes(
        &self,
        st: &FnState,
        callee: &Expr,
        argc: usize,
    ) -> Option<Vec<TypeShape>> {
        let name = match unwrap_parens(callee) {
            Expr::Ident { name, .. } => name,
            _ => return None,
        };
        if self.lookup_binding(st, name).is_some() {
            return None; // shadowed by a local binding — not a static fn call
        }
        let f = self.fns.get(name)?;
        if f.params.len() != argc {
            return None;
        }
        Some(f.params.iter().map(|p| self.shape_of_type(&p.ty)).collect())
    }

    /// The element shape of a channel expression, when it resolves (drives
    /// the N1 nil checks on sent values).
    pub(crate) fn chan_elem_shape(&self, st: &FnState, chan: &Expr) -> Option<Vec<TypeShape>> {
        match self.shape_of_expr(st, chan) {
            TypeShape::Chan(e) => Some(vec![*e]),
            _ => None,
        }
    }

    /// Classify a send's TARGET (#77): a send is only well-typed when the
    /// channel expression resolves to a channel shape. Before this check a
    /// deliberate `noop() <- 1` (void call as target) and the accidental
    /// newline fusion `noop()` / `<-done` (parser) both passed with 0
    /// diagnostics and emitted Go that failed `go build` — breaking the
    /// accepted-should-emit-buildable-Go contract. A concrete non-channel
    /// is HYB-E3007; an unresolvable shape is fail-closed HYB-E3005
    /// (O4.9). Like E3004/E3006, an ill-typed TARGET is not a sendability
    /// doubt — `managed` does not suppress it. Returns true when the
    /// target is a channel (callers then run the value check-site as
    /// usual); false emits the diagnostic and the caller skips the value
    /// check (the site is ill-typed — no cascade).
    pub(crate) fn check_send_target(&mut self, st: &FnState, chan: &Expr, span: Span) -> bool {
        match self.shape_of_expr(st, chan) {
            TypeShape::Chan(_) => true,
            TypeShape::Unknown => {
                let msg = format!(
                    "cannot prove `{}` is a channel — a send target must resolve to `chan T` (O4.9)",
                    chan.render()
                );
                let notes = vec![
                    "why: the checker cannot classify the send target's type, and a send to a non-channel is not valid Go (issue #77)".to_string(),
                    "fix: bind the channel to a local (or parameter) with a visible `chan T` type before sending, so the checker can resolve it".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3005", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
            other => {
                let msg = format!(
                    "cannot send to `{}` ({}) — a send target must be a channel (O4)",
                    chan.render(),
                    other.render()
                );
                let notes = vec![
                    "why: `<-` on a non-channel value is not valid Go; the accepted subset only emits buildable Go (issue #77)".to_string(),
                    "fix: send to a `chan T` binding (`ch := make(chan T, n)` or a `chan T` parameter); note that a statement STARTING with `<-expr` is a RECEIVE — a `<-` on a new line never continues the previous statement".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E3007", self.path.clone(), span, msg).with_notes(notes),
                );
                false
            }
        }
    }

    /// Emit HYB-E4001: `nil` flowing into a known non-nilable type (rule N1).
    /// `what` names the target in the message ("the assignment target `m`",
    /// "a parameter of the spawn callee", ...). A TYPE contract, not a
    /// sendability doubt — `managed` does not suppress it.
    pub(crate) fn emit_nil_type_error(&mut self, target: &TypeShape, span: Span, what: &str) {
        self.emit_nil_type_error_text(target, target.render(), span, what);
    }

    /// The same emission with a concrete type text (the declared ascription
    /// renders `[]int`, which is friendlier than the inferred `[]primitive`).
    pub(crate) fn emit_nil_type_error_text(
        &mut self,
        target: &TypeShape,
        ty: String,
        span: Span,
        what: &str,
    ) {
        let msg = format!(
            "`nil` is not a valid value for {} ({}): {} has no nil (N1)",
            what, ty, ty
        );
        let mut notes = vec![
            "why: nil exists only for `error` and foreign Go values (N1/N2); every Hybrid-declared type has a zero value instead (N3)".to_string(),
        ];
        notes.push(match target {
            TypeShape::Map(_, _) => "fix: use an empty map literal — a zero map reads as empty and auto-allocates on first write (N5)".to_string(),
            TypeShape::Slice(_) | TypeShape::Array(_) => "fix: use an empty slice literal (`[]T{}`) — observationally identical to Go's nil slice (N4)".to_string(),
            TypeShape::Str => "fix: use the empty string `\"\"`".to_string(),
            TypeShape::Primitive => "fix: use the type's zero value (`0`, `0.0`, `false`)".to_string(),
            TypeShape::Struct(_) => "fix: use the field-wise zero literal (`Type{}`)".to_string(),
            TypeShape::Chan(_) => "fix: declare the binding without an initializer — the zero channel is the zero value (N3)".to_string(),
            TypeShape::Pointer(_) | TypeShape::Closure => "fix: restructure so the value is always valid, or carry absence with `option[T]`".to_string(),
            _ => "fix: use the type's zero value (N3)".to_string(),
        });
        notes.push("if absence is the meaning, use `option[T]` — its zero value is `none` (N3)".to_string());
        self.diags
            .push(Diagnostic::error("HYB-E4001", self.path.clone(), span, msg).with_notes(notes));
    }

    // -----------------------------------------------------------------------
    // Check-site processing (A1c / O6 suppression + lint)
    // -----------------------------------------------------------------------

    /// Process one spawn/send check-site: classify every value, emit
    /// HYB-E3001/E3002 (A1c) or the HYB-E8001 managed lint, count for
    /// --report-managed, and mark `move` transfers.
    ///
    /// `code` is "HYB-E3001" (spawn) or "HYB-E3002" (send); `site_desc` is a
    /// self-describing phrase used in move notes ("the move-send
    /// `results <- move agg`"). `implicit_move` covers parsers that flag
    /// `Send{is_move: true}` with a plain Ident value. `spawn_callee` is the
    /// callee expression at spawn sites (the O4.0 spawn-callee proof);
    /// `target_shapes` are the per-value expected types when they resolve
    /// (the callee's parameter shapes, or the channel's element shape) and
    /// drive the N1 nil checks (HYB-E4001).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_site(
        &mut self,
        st: &mut FnState,
        span: Span,
        code: &str,
        values: &[&Expr],
        managed: bool,
        site_desc: &str,
        implicit_move: bool,
        spawn_callee: Option<&Expr>,
        target_shapes: Option<&[TypeShape]>,
    ) {
        self.total_sites += 1;

        let mut rejects: Vec<RejectInfo> = Vec::new();
        let mut unprovens: Vec<UnprovenInfo> = Vec::new();
        let mut managed_descs: Vec<String> = Vec::new();
        let mut moves: Vec<(String, Span)> = Vec::new();
        // N1 nil type errors (HYB-E4001) — emitted even inside `managed`:
        // the hatch covers sendability doubts, not the null-safety contract.
        let mut nil_rejects: Vec<(TypeShape, Span)> = Vec::new();

        // The spawn-callee proof (O4.0): classify WHAT would run in the new
        // task before looking at the arguments.
        if let Some(callee) = spawn_callee {
            match self.spawn_callee_verdict(st, callee) {
                CalleeVerdict::Proven => {}
                CalleeVerdict::Closure(info) => rejects.push(info),
                CalleeVerdict::Unproven(reason) => {
                    unprovens.push(UnprovenInfo {
                        value: callee.render(),
                        reason,
                    });
                }
            }
        }

        for (vi, v) in values.iter().enumerate() {
            // N1 (rule N): a bare `nil` is not a classifiable sendability
            // value — it either lands on a type that HAS a nil (error,
            // foreign; N2) or it is a type error (HYB-E4001). With no
            // resolvable target the fail-closed rule applies (O4.9).
            if Self::is_nil_literal(v) {
                match target_shapes.and_then(|ts| ts.get(vi)) {
                    Some(t) if Self::is_nilable_shape(t) => {}
                    Some(TypeShape::Unknown) => unprovens.push(UnprovenInfo {
                        value: "nil".to_string(),
                        reason: "the target type of this nil did not resolve, so the checker cannot prove the nil belongs here (N1: nil exists only for `error` and foreign values)".to_string(),
                    }),
                    Some(t) => nil_rejects.push((t.clone(), v.span())),
                    None => unprovens.push(UnprovenInfo {
                        value: "nil".to_string(),
                        reason: "the target type of this nil did not resolve (the channel's element type or the spawn callee's parameter), so the checker cannot prove the nil belongs here (N1)".to_string(),
                    }),
                }
                continue;
            }
            // class-3 owned moves first: `move x` (Expr form, top-level or
            // nested inside a composite literal / parens) and the parser's
            // flagged-Ident form (`Send{is_move: true, value: x}`).
            if implicit_move {
                let op = crate::model::unwrap_parens(v);
                if let Expr::Ident { name, span: ispan } = op {
                    if self.lookup_binding(st, name).is_some() {
                        if !managed {
                            self.check_non_owned_move(st, name, *ispan);
                        }
                        moves.push((name.clone(), *ispan));
                    } else {
                        unprovens.push(UnprovenInfo {
                            value: name.clone(),
                            reason: "the name did not resolve in this scope".to_string(),
                        });
                    }
                } else if !managed {
                    self.check_o54_non_ident_move(st, op, v.span());
                }
                continue;
            }

            let mut v_moves = Vec::new();
            crate::model::collect_move_operands(v, &mut v_moves);
            for (op, mspan) in v_moves {
                if let Expr::Ident { name, .. } = op {
                    if self.lookup_binding(st, name).is_some() {
                        if !managed {
                            self.check_non_owned_move(st, name, mspan);
                        }
                        moves.push((name.clone(), mspan));
                    } else {
                        unprovens.push(UnprovenInfo {
                            value: name.clone(),
                            reason: "the name did not resolve in this scope".to_string(),
                        });
                    }
                } else if !managed {
                    self.check_o54_non_ident_move(st, op, mspan);
                }
            }

            if matches!(crate::model::unwrap_parens(v), Expr::Move { .. }) {
                continue;
            }

            match self.classify_value(st, v) {
                Outcome::Ok => {}
                Outcome::Managed { desc, risky } => {
                    // only values not safe-by-type are the trust surface
                    if risky {
                        managed_descs.push(desc);
                    }
                }
                Outcome::Rejected(info) => rejects.push(info),
                Outcome::Unproven(info) => unprovens.push(info),
            }
        }

        if !rejects.is_empty() || !unprovens.is_empty() {
            if managed {
                // O6.1: sites inside a managed block are permitted + linted —
                // this is the explicit, auditable escape hatch that covers
                // both rejected shapes and (O4.9) unproven ones: what the
                // hatch cannot do is make the checker CALL it safe.
                self.sites_via_managed += 1;
                let mut shared: Vec<String> = rejects
                    .iter()
                    .map(|r| format!("`{}` ({})", r.value, r.ty))
                    .collect();
                shared.extend(
                    unprovens
                        .iter()
                        .map(|u| format!("`{}` (unproven — {})", u.value, u.reason)),
                );
                let msg = format!(
                    "spawn/send site inside a `managed` block: {} shared across tasks (unchecked)",
                    shared.join(", ")
                );
                let notes = vec![
                    "the `managed` hatch suppresses the sendability check (O6.1): the contract is Go's DRF-SC discipline with `-race` as the backstop".to_string(),
                    "managed does not remove Go's own runtime fatals (concurrent map writes are still a runtime fatal) — prefer the channel redesign or add a lock".to_string(),
                    "counted by --report-managed — the hatch is greppable, auditable, countable (O6.3)".to_string(),
                ];
                self.diags.push(
                    Diagnostic::warning("HYB-E8001", self.path.clone(), span, msg)
                        .with_notes(notes),
                );
            } else {
                let site_word = if code == "HYB-E3001" {
                    "across tasks"
                } else {
                    "at this channel send"
                };
                for r in rejects {
                    let msg = format!("`{}` ({}) is not sendable {}", r.value, r.ty, site_word);
                    self.diags.push(
                        Diagnostic::error(code, self.path.clone(), span, msg).with_notes(r.notes),
                    );
                }
                // O4.9 fail-closed (rule FC): unproven values never silently
                // pass. Outside a managed block the site does not build until
                // the value's type is visible to the checker — or the hatch
                // is taken deliberately. The diagnostic teaches the fix (O17).
                for u in unprovens {
                    let msg = format!(
                        "`{}` cannot be proven safe {} — its type did not resolve, so the checker cannot verify the share",
                        u.value, site_word
                    );
                    let notes = vec![
                        format!("why: {}", u.reason),
                        "the checker is fail-closed at boundaries: what it cannot prove, it will not call safe (O4.9) — a wrong silent pass here would void the Tier-0 guarantee for the whole program".to_string(),
                        "fix: ascribe the type at the declaration (`var x map[string]int = ...`) or route the value through a channel of a known sendable element type".to_string(),
                        "if the share is genuinely guarded, wrap this site in `managed(\"why this bypass is safe\", \"category\") { ... }` — the explicit, auditable hatch (HYB-E8001, counted by --report-managed; categories: mutex, atomic, actor, finalizer, interop, other)".to_string(),
                    ];
                    self.diags.push(
                        Diagnostic::error("HYB-E3005", self.path.clone(), span, msg)
                            .with_notes(notes),
                    );
                }
            }
        } else if !managed_descs.is_empty() {
            // class 6 resolution (managed-tainted value), even outside a
            // block; only non-safe-by-type values are the trust surface
            self.sites_via_managed += 1;
            let msg = format!(
                "managed value crosses this spawn/send site (class 6 — outside the safety guarantee): {}",
                managed_descs.join(", ")
            );
            let notes = vec![
                "a value born inside a `managed` block is sendable everywhere but stays outside the §9.2 guarantee for its whole lifetime (O6.2)".to_string(),
                "counted by --report-managed (O6.3)".to_string(),
            ];
            self.diags.push(
                Diagnostic::warning("HYB-E8001", self.path.clone(), span, msg).with_notes(notes),
            );
        }

        // N1 (rule N): nil-into-non-nilable is a TYPE error — it fires even
        // inside a `managed` block (the hatch covers sendability doubts, not
        // the null-safety contract).
        let nil_what = if code == "HYB-E3001" {
            "a parameter of the spawn callee"
        } else {
            "the channel's element type"
        };
        for (t, nspan) in nil_rejects {
            self.emit_nil_type_error(&t, nspan, nil_what);
        }

        // O5.1: mark moved bindings (+ freeze live aliases)
        for (name, mspan) in moves {
            self.mark_move(st, &name, mspan, site_desc.to_string(), MoveKind::Moved);
        }
    }
}

fn unproven_atomic_reason(b: &Binding) -> Option<String> {
    let ty = b.ty.as_ref()?.render();
    let head = ty.split('[').next().unwrap_or(ty.as_str());
    if crate::foreign::is_unproven_atomic(head) {
        Some(format!(
            "`{ty}` is not in the Tier-0 scalar atomic set (GUARANTEES.md row 11: `atomic.pointer[T]` / `atomic.value` publish a reference to arbitrary state without proving the pointee is race-free — unproven → rejected)"
        ))
    } else {
        None
    }
}

