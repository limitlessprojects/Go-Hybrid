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
use hy_syntax::{Expr, LitKind, TypeExpr};

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

    fn classify_move_ident(&self, st: &FnState, name: &str) -> Outcome {
        let Some(b) = self.lookup_binding(st, name) else {
            // Unresolved move operand is already recorded by the move-operand pass.
            return Outcome::Ok;
        };
        if b.moved.is_some() {
            return Outcome::Ok;
        }
        if b.managed_taint {
            return Outcome::Managed {
                desc: self.managed_desc(name, b),
                risky: !b.shape.is_sendable_by_type(),
            };
        }
        match &b.shape {
            TypeShape::Unknown => Outcome::Unproven(UnprovenInfo {
                value: name.to_string(),
                reason: unproven_atomic_reason(b).unwrap_or_else(|| {
                    "its type did not resolve to a sendability class (no ascription and the initializer's type was unresolvable)".to_string()
                }),
            }),
            TypeShape::Closure => Outcome::Rejected(RejectInfo {
                value: name.to_string(),
                ty: self.binding_ty_str(b),
                notes: vec![
                    "closures capture their environment by reference; moving a closure binding does not move or freeze its captured state, so closures are not sendable at spawn/send sites in Tier 0 (A5 perimeter)".to_string(),
                ],
            }),
            TypeShape::BorrowHandle(_) => Outcome::Rejected(RejectInfo {
                value: name.to_string(),
                ty: self.binding_ty_str(b),
                notes: vec![format!(
                    "`{}` is a borrow handle (element view) — borrow loans are function-local and cannot be moved across tasks (O2/O7/O11); only own-handles transfer via `move`",
                    name
                )],
            }),
            TypeShape::ForeignOpaque(_) => Outcome::Rejected(RejectInfo {
                value: name.to_string(),
                ty: self.binding_ty_str(b),
                notes: vec![format!(
                    "`{}` is an opaque foreign value ({}) — foreign runtime/synchronization types cannot be moved across tasks (O4/A5)",
                    name,
                    self.binding_ty_str(b)
                )],
            }),
            shape if self.shape_holds_closure(shape, &mut Vec::new()) => {
                Outcome::Rejected(RejectInfo {
                    value: name.to_string(),
                    ty: self.binding_ty_str(b),
                    notes: vec![format!(
                        "`{}` is a {} that holds closures; moving the container moves only the closure values, not the environment they capture, so the captured state stays reachable from this task (A5 perimeter, same rule as moving a bare closure)",
                        name,
                        shape.noun()
                    )],
                })
            }
            shape if self.shape_holds_unmovable_leaf(shape, &mut Vec::new()) => {
                Outcome::Rejected(RejectInfo {
                    value: name.to_string(),
                    ty: self.binding_ty_str(b),
                    notes: vec![format!(
                        "`{}` is a {} that holds a borrow handle or foreign opaque value; moving the container cannot transfer non-movable interior state across tasks (O4/O7/A5)",
                        name,
                        shape.noun()
                    )],
                })
            }
            _ => Outcome::Ok,
        }
    }

    /// True when a composite shape (slice / array / map / struct / pointer)
    /// reaches a `Closure` through its elements or fields. A moved container
    /// of closures would otherwise launder captured state past the freeze:
    /// `move fs` freezes `fs`, never the bindings the closures inside it
    /// capture. `seen` guards recursive struct decls.
    pub(crate) fn shape_holds_closure(&self, shape: &TypeShape, seen: &mut Vec<String>) -> bool {
        match shape {
            TypeShape::Closure => true,
            TypeShape::Slice(e) | TypeShape::Array(e) | TypeShape::Pointer(e) => {
                self.shape_holds_closure(e, seen)
            }
            TypeShape::Map(k, v) => {
                self.shape_holds_closure(k, seen) || self.shape_holds_closure(v, seen)
            }
            TypeShape::Struct(Some(sname)) => {
                if seen.iter().any(|s| s == sname) {
                    return false;
                }
                seen.push(sname.clone());
                let Some(fields) = self.structs.get(sname) else {
                    return false;
                };
                fields
                    .iter()
                    .any(|(_, ty)| self.shape_holds_closure(&self.shape_of_type(ty), seen))
            }
            _ => false,
        }
    }

    /// True when a composite shape reaches a `BorrowHandle` or `ForeignOpaque`
    /// through its elements or fields (neither can cross tasks via `move`).
    pub(crate) fn shape_holds_unmovable_leaf(
        &self,
        shape: &TypeShape,
        seen: &mut Vec<String>,
    ) -> bool {
        match shape {
            TypeShape::BorrowHandle(_) | TypeShape::ForeignOpaque(_) => true,
            TypeShape::Slice(e) | TypeShape::Array(e) | TypeShape::Pointer(e) => {
                self.shape_holds_unmovable_leaf(e, seen)
            }
            TypeShape::Map(k, v) => {
                self.shape_holds_unmovable_leaf(k, seen)
                    || self.shape_holds_unmovable_leaf(v, seen)
            }
            TypeShape::Struct(Some(sname)) => {
                if seen.iter().any(|s| s == sname) {
                    return false;
                }
                seen.push(sname.clone());
                let Some(fields) = self.structs.get(sname) else {
                    return false;
                };
                fields
                    .iter()
                    .any(|(_, ty)| self.shape_holds_unmovable_leaf(&self.shape_of_type(ty), seen))
            }
            _ => false,
        }
    }

    /// True when a `const` value of `shape` is a pure value-copy representation
    /// with no shared reference backing, no closure captures, no handles, and
    /// no foreign synchronization/opaque state (Class-4 immutability exemption).
    pub(crate) fn shape_is_const_sendable(
        &self,
        shape: &TypeShape,
        seen: &mut Vec<String>,
    ) -> bool {
        match shape {
            TypeShape::Primitive | TypeShape::Str | TypeShape::Atomic | TypeShape::Chan(_) => true,
            TypeShape::Array(elem) => self.shape_is_const_sendable(elem, seen),
            TypeShape::Struct(Some(sname)) => {
                if seen.iter().any(|s| s == sname) {
                    return false;
                }
                seen.push(sname.clone());
                let Some(fields) = self.structs.get(sname) else {
                    return false;
                };
                fields
                    .iter()
                    .all(|(_, ty)| self.shape_is_const_sendable(&self.shape_of_type(ty), seen))
            }
            _ => false,
        }
    }

    pub(crate) fn classify_value(&self, st: &FnState, expr: &Expr) -> Outcome {
        // `move x` — class 3; validate that `x` resolves to a movable shape
        // (never Unknown, Closure, BorrowHandle, or ForeignOpaque).
        if let Expr::Move { operand, .. } = unwrap_parens(expr) {
            if let Expr::Ident { name, .. } = unwrap_parens(operand) {
                return self.classify_move_ident(st, name);
            }
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
            Expr::Call { callee, args, .. }
                if matches!(
                    unwrap_parens(callee),
                    Expr::Ident { name, .. } if name == "some" && self.lookup_binding(st, name).is_none()
                ) =>
            {
                self.classify_composite(st, expr, args.iter().map(|a| &a.value))
            }
            // field/index/slice reads: classify the read's shape, but honor
            // immutable roots when the resulting value is a value copy (views
            // of const values that do not share reference storage stay
            // immutable, OQ5; slice views or reference fields of a const
            // composite share mutable Go runtime storage, #188).
            Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
                let shape = self.shape_of_expr(st, expr);
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
                        if b.immutable && self.shape_is_const_sendable(&shape, &mut Vec::new()) {
                            return Outcome::Ok;
                        }
                    }
                }
                self.outcome_for_shape(&shape, &expr.render())
            }
            // everything else (calls, receives, arithmetic, try, paren):
            // classify the result shape
            _ => {
                let unwrapped = unwrap_parens(expr);
                let (call_expr, is_try) = match unwrapped {
                    Expr::Try { operand, .. } => (unwrap_parens(operand), true),
                    other => (other, false),
                };
                if let Expr::Call { callee, .. } = call_expr {
                    if let Some(results) = self.call_result_tys(st, callee) {
                        let avail = if is_try {
                            results.len().saturating_sub(1)
                        } else {
                            results.len()
                        };
                        if avail >= 2 {
                            for ty in &results[..avail] {
                                let shape = self.shape_of_type(ty);
                                let outcome = self.outcome_for_shape(&shape, &expr.render());
                                if !matches!(outcome, Outcome::Ok) {
                                    return outcome;
                                }
                            }
                            return Outcome::Ok;
                        }
                    }
                }
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
        // Issue #188: `const` composites carrying reference storage (maps,
        // slices, pointers, or structs/arrays containing them), closures,
        // handles, or foreign opaques lower to runtime Go `var`s or capture
        // mutable state. Only `const` bindings whose runtime representation is
        // a pure value copy qualify for the Class-4 immutability exemption at
        // spawn/send sites.
        if b.immutable && self.shape_is_const_sendable(&b.shape, &mut Vec::new()) {
            return Outcome::Ok; // class 4 (value-copy const)
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
        if b.immutable {
            notes.push(
                "a composite `const` with reference storage lowers to a runtime-mutable Go value, so sharing it across a task or channel boundary without `managed` can race if the receiver mutates it (issue #188)".to_string(),
            );
        }
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
            // Address-of produces a pointer even when its storage has no named root.
            let shape = self.shape_of_expr(st, operand);
            if matches!(shape, TypeShape::Unknown) {
                return self.outcome_for_shape(&TypeShape::Unknown, &expr.render());
            }
            let ptr_shape = TypeShape::Pointer(Box::new(shape));
            if matches!(&ptr_shape, TypeShape::Pointer(inner) if matches!(**inner, TypeShape::Chan(_))) {
                return Outcome::Ok;
            }
            return self.outcome_for_shape(&ptr_shape, &expr.render());
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
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if self.lookup_binding(st, name).is_some() {
                    return None; // shadowed by a local binding — not a static fn call
                }
                let f = self.fns.get(name)?;
                if f.params.len() != argc {
                    return None;
                }
                Some(f.params.iter().map(|p| self.shape_of_type(&p.ty)).collect())
            }
            Expr::FnLit { params, .. } => {
                if params.len() != argc {
                    return None;
                }
                Some(params.iter().map(|p| self.shape_of_type(&p.ty)).collect())
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                let recv_base = match self.shape_of_expr(st, base) {
                    TypeShape::Struct(Some(s)) => Some(s),
                    TypeShape::Pointer(inner) => match *inner {
                        TypeShape::Struct(Some(s)) => Some(s),
                        _ => None,
                    },
                    _ => None,
                };
                let f = recv_base.and_then(|rb| {
                    self.method_decls.get(&(rb, method_name.clone())).copied()
                })?;
                if f.params.len() != argc {
                    return None;
                }
                Some(f.params.iter().map(|p| self.shape_of_type(&p.ty)).collect())
            }
            _ => None,
        }
    }

    /// Could a value of shape `got` legitimately occupy a slot of shape
    /// `want` (channel element on send, or typed target on receive)?
    /// Compares COARSE TypeShape classes only. `Unknown` / unnamed struct /
    /// foreign-opaque never reject — a false reject is worse than a miss
    /// (issue #146).
    pub(crate) fn shape_may_be_elem(got: &TypeShape, want: &TypeShape) -> bool {
        use TypeShape::*;
        match (got, want) {
            (Unknown, _) | (_, Unknown) => true,
            (Struct(None), _) | (_, Struct(None)) => true,
            (ForeignOpaque(_), _) | (_, ForeignOpaque(_)) => true,
            (Primitive, Primitive) | (Str, Str) | (Atomic, Atomic) => true,
            (Struct(Some(a)), Struct(Some(b))) => a == b,
            (Map(_, _), Map(_, _)) => true,
            (Slice(_), Slice(_)) | (Array(_), Array(_)) => true,
            // Go converts between slice and array of the same element class
            // only in limited cases; keep coarse-class permissive here.
            (Slice(_), Array(_)) | (Array(_), Slice(_)) => true,
            (Pointer(_), Pointer(_)) => true,
            (Chan(a), Chan(b)) => Self::shape_may_be_elem(a, b),
            (Closure, Closure) => true,
            (OwnHandle(a), OwnHandle(b)) | (BorrowHandle(a), BorrowHandle(b)) => a == b,
            _ => false,
        }
    }

    /// Human label for an element/target shape in HYB-E3011 messages.
    pub(crate) fn elem_type_label(t: &TypeShape) -> String {
        match t {
            TypeShape::Struct(Some(name)) => format!("`{name}`"),
            TypeShape::ForeignOpaque(name)
            | TypeShape::OwnHandle(name)
            | TypeShape::BorrowHandle(name) => format!("`{name}`"),
            other => format!("`{}`", other.render()),
        }
    }

    /// Emit HYB-E3011 for a send whose value shape cannot be the channel
    /// element. A TYPE contract — `managed` does not suppress it.
    pub(crate) fn emit_chan_elem_mismatch_send(
        &mut self,
        value_shape: &TypeShape,
        elem_shape: &TypeShape,
        span: Span,
        value_desc: &str,
    ) {
        let msg = format!(
            "{} has type {} but the channel element type is {} (HYB-E3011)",
            value_desc,
            Self::elem_type_label(value_shape),
            Self::elem_type_label(elem_shape)
        );
        self.push_chan_elem_mismatch(span, msg);
    }

    /// Emit HYB-E3011 for a typed receive whose target cannot hold the
    /// channel element.
    pub(crate) fn emit_chan_elem_mismatch_recv(
        &mut self,
        target_shape: &TypeShape,
        elem_shape: &TypeShape,
        span: Span,
        target_desc: &str,
    ) {
        let msg = format!(
            "{} expects type {} but the channel element type is {} (HYB-E3011)",
            target_desc,
            Self::elem_type_label(target_shape),
            Self::elem_type_label(elem_shape)
        );
        self.push_chan_elem_mismatch(span, msg);
    }

    fn push_chan_elem_mismatch(&mut self, span: Span, msg: String) {
        let notes = vec![
            "why: the accepted subset only emits buildable Go; a mismatched channel element fails `go build` with `cannot use ... as ... value in send` / assignment (issue #146)".to_string(),
            "fix: send/receive a value of the declared element type, or change the channel's element type to match".to_string(),
            "note: channel DIRECTION is checked separately (HYB-E3007); this is the ELEMENT TYPE".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3011", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// Receive site: when RHS is `<-ch` (or a paren-wrapped receive) and the
    /// target shape is known, reject a certain element/target class mismatch.
    pub(crate) fn check_recv_elem_against_target(
        &mut self,
        st: &FnState,
        value: &Expr,
        target: &TypeShape,
        span: Span,
        what: &str,
    ) {
        if matches!(target, TypeShape::Unknown) {
            return;
        }
        let recv = crate::model::unwrap_parens(value);
        let Expr::Receive { chan, .. } = recv else {
            return;
        };
        let TypeShape::Chan(elem) = self.shape_of_expr(st, chan) else {
            return;
        };
        if matches!(*elem, TypeShape::Unknown) {
            return;
        }
        if !Self::shape_may_be_elem(&elem, target) {
            self.emit_chan_elem_mismatch_recv(target, &elem, span, what);
        }
    }

    /// The element shape of a channel expression, when it resolves (drives
    /// the N1 nil checks on sent values).
    pub(crate) fn chan_elem_shape(&self, st: &FnState, chan: &Expr) -> Option<Vec<TypeShape>> {
        match self.shape_of_expr(st, chan) {
            TypeShape::Chan(e) => Some(vec![*e]),
            _ => None,
        }
    }

    /// The element type of a channel expression, when it resolves.
    pub(crate) fn chan_elem_ty(&self, st: &FnState, chan: &Expr) -> Option<TypeExpr> {
        let ty = self.ty_of_expr(st, chan)?;
        let resolved = self.resolve_type_alias(&ty);
        match resolved {
            TypeExpr::Chan { elem, .. } => Some(*elem),
            _ => None,
        }
    }

    /// Check for constant overflow or float truncation in a channel send.
    pub(crate) fn check_chan_send_const_overflow(
        &mut self,
        st: &FnState,
        chan: &Expr,
        value: &Expr,
    ) {
        if let Some(elem_ty) = self.chan_elem_ty(st, chan) {
            let desc = format!("channel `{}`", chan.render());
            self.check_const_overflow_init(&desc, &elem_ty, value, Some(st));
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
            TypeShape::Chan(_) => {
                if let Some(ty) = self.ty_of_expr(st, chan) {
                    let resolved = self.resolve_type_alias(&ty);
                    if matches!(
                        resolved,
                        hy_syntax::TypeExpr::Chan {
                            dir: hy_syntax::ChanDir::Recv,
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
                                "cannot send to `{}` (`{}`) — channel is receive-only (HYB-E3007)",
                                chan.render(),
                                resolved.render()
                            );
                            let notes = vec![
                                "why: Go rejects `ch <- v` on a `<-chan T` receive-only channel (`invalid operation: cannot send to receive-only channel`), so an accepted Hybrid program would emit unbuildable Go".to_string(),
                                "fix: declare the parameter or binding as a bidirectional (`chan T`) or send-only (`chan<- T`) channel".to_string(),
                            ];
                            self.diags.push(
                                Diagnostic::error("HYB-E3007", self.path.clone(), span, msg)
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

    /// Emit HYB-E4003: an UNTYPED nil declaration — `var x = nil` / `x := nil`
    /// / `let x = nil` (no ascription). The E4001 sites all require a declared
    /// type to prove the nil against; with no ascription there is no type at
    /// all, `shape_of_expr(nil)` is Unknown, and the checker used to accept the
    /// declaration — then the emitter emitted `var x = nil`, which Go rejects
    /// outright (`use of untyped nil in variable declaration`). nil has exactly
    /// two homes (N1): `error` and the foreign-Go perimeter, and both must be
    /// visible in a DECLARED type — an untyped nil can never name either.
    pub(crate) fn emit_untyped_nil_decl_error(&mut self, name: &str, span: Span) {
        let msg = format!(
            "`{}` is declared from an untyped `nil`: Go rejects the emitted declaration (`use of untyped nil in variable declaration`)",
            name
        );
        let notes = vec![
            "why: nil has exactly two homes (N1) — `error` and the foreign-Go perimeter (N2) — and both must be visible in a DECLARED type; an untyped nil can never name either, so the emitted `var x = nil` fails `go build`".to_string(),
            "fix: declare the nil home explicitly — `var e error = nil` (error's zero value) or `var h Foreign.handle = nil` at the foreign perimeter (N1/N2)".to_string(),
            "fix: declare the binding without an initializer — the zero value of the intended type IS the value (N3): `var ch chan int` (zero channel), `var s []int` (zero slice), `var m map[string]int` (zero map)".to_string(),
            "fix: if absence is the meaning, use `option[T]` — its zero value is `none` (N3)".to_string(),
        ];
        self.diags
            .push(Diagnostic::error("HYB-E4003", self.path.clone(), span, msg).with_notes(notes));
    }

    /// N1 for composite literals: `job{m: nil}` with a map-typed field used
    /// to pass the checker — classify_composite has no per-field type
    /// context, so the bare-nil check at the five direct-flow sites never
    /// saw the nested `nil`. The fix follows the N1 doctrine (a TYPE
    /// contract, emitted even inside `managed`): every literal carries its
    /// own type context (the struct def, the map's value type, the slice's
    /// element type), so a `nil` whose slot resolves to a non-nilable shape
    /// is a type error right there — boundary or not.
    pub(crate) fn check_composite_nils(&mut self, st: &FnState, value: &Expr) {
        match unwrap_parens(value) {
            Expr::StructLit { ty, fields, .. } => {
                // the literal's own ascription wins; else infer from the env
                // (an ascribed define `var j jobT = jobT{...}` names the
                // struct twice — same answer)
                let sname = ty.as_ref().and_then(|t| match t {
                    hy_syntax::TypeExpr::Named { name, .. } => Some(name.clone()),
                    _ => None,
                });
                // A struct literal spelled through a type alias (`type
                // holder_alias = holder; h := holder_alias{m: nil}`) names the
                // ALIAS here, but `self.structs` is keyed by the real
                // declaration name — the raw lookup missed, and the nil-field
                // sweep silently skipped every field of an alias-spelled
                // literal while the same literal spelled directly rejected
                // (#385, live probe 2026-10-18). Follow the alias chain to its
                // root before the lookup; non-struct aliases resolve to a name
                // that is simply absent from `structs`, same as before.
                let sname = sname.map(|n| {
                    let mut cur = n;
                    for _ in 0..8 {
                        match self.type_aliases.get(cur.as_str()) {
                            Some(hy_syntax::TypeExpr::Named { name, .. }) if name != &cur => {
                                cur = name.clone();
                            }
                            _ => break,
                        }
                    }
                    cur
                });
                if let Some(sname) = sname.or_else(|| {
                    let shape = self.shape_of_expr(st, value);
                    match shape {
                        TypeShape::Struct(Some(n)) => Some(n),
                        _ => None,
                    }
                }) {
                    if let Some(def) = self.structs.get(&sname).cloned() {
                        for f in fields {
                            if let Some((_, fty)) = def.iter().find(|(n, _)| n == &f.name) {
                                if Checker::is_nil_literal(&f.value) {
                                    let fshape = self.shape_of_type(fty);
                                    if !Checker::is_nilable_shape(&fshape) {
                                        self.emit_nil_type_error_text(
                                            &fshape,
                                            fty.render(),
                                            f.value.span(),
                                            &format!("the field `{}.{}`", sname, f.name),
                                        );
                                    }
                                    continue;
                                }
                                self.check_composite_nils(st, &f.value);
                            }
                        }
                    }
                }
            }
            Expr::MapLit { value: vty, entries, .. } => {
                let vshape = self.shape_of_type(vty);
                let vty_text = vty.render();
                for (k, v) in entries {
                    if Checker::is_nil_literal(v) && !Checker::is_nilable_shape(&vshape) {
                        self.emit_nil_type_error_text(
                            &vshape,
                            vty_text.clone(),
                            v.span(),
                            &format!("the map value slot `{}`", vty_text),
                        );
                        continue;
                    }
                    self.check_composite_nils(st, v);
                    self.check_composite_nils(st, k);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                let elem_ty = value
                    .as_array_lit()
                    .map(|(_, inner_elem, _)| inner_elem)
                    .or(elem.as_deref());
                if let Some(ety) = elem_ty {
                    self.check_composite_nils_items(st, ety, items);
                } else {
                    for it in items {
                        self.check_composite_nils(st, it);
                    }
                }
            }
            _ => {}
        }
    }

    fn check_composite_nils_items(&mut self, st: &FnState, ety: &TypeExpr, items: &[Expr]) {
        let eshape = self.shape_of_type(ety);
        let ety_text = ety.render();
        for it in items {
            if Checker::is_nil_literal(it) && !Checker::is_nilable_shape(&eshape) {
                self.emit_nil_type_error_text(
                    &eshape,
                    ety_text.clone(),
                    it.span(),
                    &format!("the element slot `{}`", ety_text),
                );
                continue;
            }
            if let Expr::SliceLit {
                elem: None,
                items: sub_items,
                ..
            } = unwrap_parens(it)
            {
                let resolved = self.resolve_type_alias(ety);
                if let TypeExpr::Slice { elem: sub_ety, .. }
                | TypeExpr::Array { elem: sub_ety, .. } = &resolved
                {
                    self.check_composite_nils_items(st, sub_ety, sub_items);
                    continue;
                }
            }
            self.check_composite_nils(st, it);
        }
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
            // N1 for composite literals: a literal that crosses the boundary
            // with nested nils (`task worker(jobT{m: nil})`) is a TYPE error
            // at its own slot — emitted even inside `managed` (HYB-E4001).
            self.check_composite_nils(st, v);
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
                } else if matches!(op, Expr::Field { .. })
                    && matches!(self.shape_of_expr(st, op), TypeShape::OwnHandle(_))
                {
                    if let Some(root) = op.root_ident() {
                        if self.lookup_binding(st, root).is_some() {
                            if !managed {
                                self.check_non_owned_move(st, root, v.span());
                            }
                            moves.push((root.to_string(), v.span()));
                        }
                    }
                } else if !managed {
                    self.check_o54_non_ident_move(st, op, v.span());
                }
                if let Expr::Ident { name, .. } = op {
                    match self.classify_move_ident(st, name) {
                        Outcome::Ok => {}
                        Outcome::Managed { desc, risky } => {
                            if risky {
                                managed_descs.push(desc);
                            }
                        }
                        Outcome::Rejected(info) => rejects.push(info),
                        Outcome::Unproven(info) => unprovens.push(info),
                    }
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
                } else if matches!(op, Expr::Field { .. })
                    && matches!(self.shape_of_expr(st, op), TypeShape::OwnHandle(_))
                {
                    if let Some(root) = op.root_ident() {
                        if self.lookup_binding(st, root).is_some() {
                            if !managed {
                                self.check_non_owned_move(st, root, mspan);
                            }
                            moves.push((root.to_string(), mspan));
                        }
                    }
                } else if !managed {
                    self.check_o54_non_ident_move(st, op, mspan);
                }
            }

            // Issue #146: once sendability accepts the value, still reject a
            // CERTAIN channel-element type mismatch (send only — spawn has
            // no element type). Reported only on Outcome::Ok so a value
            // already rejected as non-sendable (HYB-E3002) keeps a single
            // diagnostic (no cascade).
            let send_elem_mismatch = if code == "HYB-E3002" {
                target_shapes
                    .and_then(|ts| ts.get(vi))
                    .cloned()
                    .and_then(|want| {
                        let got = self.shape_of_expr(st, v);
                        if matches!(got, TypeShape::Unknown) {
                            None
                        } else if Self::shape_may_be_elem(&got, &want) {
                            None
                        } else {
                            Some((got, want))
                        }
                    })
            } else {
                None
            };

            let is_auto_addressed = code == "HYB-E3001"
                && matches!(target_shapes.and_then(|ts| ts.get(vi)), Some(TypeShape::Pointer(_)))
                && !matches!(self.shape_of_expr(st, v), TypeShape::Pointer(_))
                && matches!(
                    crate::model::unwrap_parens(v),
                    Expr::Ident { .. } | Expr::Field { .. } | Expr::Index { .. }
                );

            let outcome = if is_auto_addressed {
                self.classify_addr_of(st, v, crate::model::unwrap_parens(v))
            } else {
                self.classify_value(st, v)
            };

            match outcome {
                Outcome::Ok => {
                    if let Some((got, want)) = send_elem_mismatch {
                        self.emit_chan_elem_mismatch_send(
                            &got,
                            &want,
                            v.span(),
                            &format!("`{}`", v.render()),
                        );
                    }
                }
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

    // -----------------------------------------------------------------------
    // Channel direction enforcement for `close()` and `for … in` (HYB-E3009, #94)
    // -----------------------------------------------------------------------

    /// Triggered once per file from Pass B (`arith.rs`): walks every declared
    /// function with lexical type tracking that preserves narrowing channel
    /// ascriptions (`var rc <-chan int = c`) and enforces:
    ///   1. `close(ch)` (including `defer close(ch)` and calls inside closures)
    ///      rejects `<-chan T` (`ChanDir::Recv`) with `HYB-E3009`.
    ///   2. `for … in ch` rejects `chan<- T` (`ChanDir::Send`) with `HYB-E3009`.
    ///   3. Value-flow coercion sites (`var`/`const` ascription, assignment,
    ///      call/task/method/closure args, return, struct/slice/map literal
    ///      fields/elements, and channel sends/receives of channel elements)
    ///      reject unidirectional channel direction mismatches with `HYB-E3012`
    ///      (issue #623).
    pub(crate) fn check_file_close_and_range_directions(&mut self, trigger_stmt: &hy_syntax::Stmt) {
        let first_span = self
            .file
            .fns()
            .find_map(|f| f.body.as_ref()?.stmts.first().map(|s| s.span()));
        if first_span != Some(trigger_stmt.span()) {
            return;
        }
        let mut const_env = ChanDirEnv::new();
        let const_decls: Vec<&'a hy_syntax::ConstDecl> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                hy_syntax::Decl::Const(c) => Some(c),
                _ => None,
            })
            .collect();
        for c in const_decls {
            self.walk_expr_chan_dir(&c.value, &mut const_env);
            if let Some(want_ty) = &c.ty {
                self.check_expr_against_ty(&c.value, want_ty, &const_env);
                const_env.define(&c.name, Some(want_ty.clone()));
            } else {
                let inferred = self.dir_ty_of_expr(&c.value, &const_env);
                const_env.define(&c.name, inferred);
            }
        }
        let fns: Vec<&'a hy_syntax::FnDecl> = self.file.fns().collect();
        for f in fns {
            self.check_fn_close_and_range_directions(f, &const_env);
        }
    }

    fn check_fn_close_and_range_directions(
        &mut self,
        f: &hy_syntax::FnDecl,
        const_env: &ChanDirEnv,
    ) {
        let Some(body) = &f.body else {
            return;
        };
        let mut env = const_env.clone();
        env.push();
        env.push_results(f.results.iter().map(|r| r.ty.clone()).collect());
        if let Some(r) = &f.receiver {
            env.define(&r.name, Some(r.ty.clone()));
        }
        for p in &f.params {
            env.define(&p.name, Some(p.ty.clone()));
        }
        for res in &f.results {
            if let Some(name) = &res.name {
                env.define(name, Some(res.ty.clone()));
            }
        }
        for stmt in &body.stmts {
            self.walk_stmt_chan_dir(stmt, &mut env);
        }
        env.pop_results();
        env.pop();
    }

    fn walk_stmt_chan_dir(&mut self, stmt: &hy_syntax::Stmt, env: &mut ChanDirEnv) {
        use hy_syntax::{ForKind, LocalKind, MatchPattern, SelectArmKind, Stmt, TypeExpr};
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.walk_expr_chan_dir(v, env);
                }
                if let Some(ty) = &l.ty {
                    if l.values.len() == 1 && l.names.len() > 1 {
                        if let Some(got_tys) = self.dir_multi_ty_of_call(&l.values[0], env) {
                            for got_ty in &got_tys {
                                self.check_chan_dir_coercion(
                                    got_ty,
                                    ty,
                                    l.values[0].span(),
                                    &l.values[0].render(),
                                );
                            }
                        }
                    } else {
                        for v in &l.values {
                            self.check_expr_against_ty(v, ty, env);
                        }
                    }
                    for name in &l.names {
                        if name != "_" {
                            env.define(name, Some(ty.clone()));
                        }
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let multi = self.dir_multi_ty_of_call(&l.values[0], env);
                    for (i, name) in l.names.iter().enumerate() {
                        if name != "_" {
                            let ty = multi.as_ref().and_then(|ts| ts.get(i).cloned());
                            if l.kind == LocalKind::Define {
                                if let Some(Some(existing_ty)) = env.lookup_current_scope(name) {
                                    if let Some(got_ty) = &ty {
                                        self.check_chan_dir_coercion(
                                            got_ty,
                                            &existing_ty,
                                            l.values[0].span(),
                                            &l.values[0].render(),
                                        );
                                    }
                                    continue;
                                }
                            }
                            env.define(name, ty);
                        }
                    }
                } else {
                    for (i, name) in l.names.iter().enumerate() {
                        if name != "_" {
                            let ty = l.values.get(i).and_then(|v| self.dir_ty_of_expr(v, env));
                            if l.kind == LocalKind::Define && l.names.len() > 1 {
                                if let Some(Some(existing_ty)) = env.lookup_current_scope(name) {
                                    if let Some(v) = l.values.get(i) {
                                        self.check_expr_against_ty(v, &existing_ty, env);
                                    }
                                    continue;
                                }
                            }
                            env.define(name, ty);
                        }
                    }
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.walk_place_chan_dir(t, env);
                }
                for v in values {
                    self.walk_expr_chan_dir(v, env);
                }
                if values.len() == 1 && targets.len() > 1 {
                    if let Some(got_tys) = self.dir_multi_ty_of_call(&values[0], env) {
                        for (t, got_ty) in targets.iter().zip(got_tys.iter()) {
                            if let Some(want_ty) = self.dir_ty_of_place(t, env) {
                                self.check_chan_dir_coercion(
                                    got_ty,
                                    &want_ty,
                                    values[0].span(),
                                    &values[0].render(),
                                );
                            }
                        }
                    }
                } else {
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let Some(want_ty) = self.dir_ty_of_place(t, env) {
                            self.check_expr_against_ty(v, &want_ty, env);
                        }
                    }
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.walk_expr_chan_dir(expr, env);
            }
            Stmt::Send {
                chan, value, span, ..
            } => {
                if matches!(
                    self.chan_dir_of_expr_env(chan, env),
                    Some(hy_syntax::ChanDir::Recv)
                ) {
                    self.emit_send_recv_only(chan, *span);
                }
                if let Some(chan_ty) = self.dir_ty_of_expr(chan, env) {
                    if let TypeExpr::Chan {
                        elem: want_elem, ..
                    } = self.resolve_type_alias(&chan_ty)
                    {
                        self.check_expr_against_ty(value, &want_elem, env);
                    }
                }
                self.walk_expr_chan_dir(chan, env);
                self.walk_expr_chan_dir(value, env);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                env.push();
                if let Some(init) = init {
                    self.walk_stmt_chan_dir(init, env);
                }
                self.walk_expr_chan_dir(cond, env);
                env.push();
                for s in &then.stmts {
                    self.walk_stmt_chan_dir(s, env);
                }
                env.pop();
                if let Some(other) = otherwise {
                    env.push();
                    self.walk_stmt_chan_dir(other, env);
                    env.pop();
                }
                env.pop();
            }
            Stmt::For { kind, body, .. } => {
                env.push();
                match kind {
                    ForKind::While { cond } => self.walk_expr_chan_dir(cond, env),
                    ForKind::Range { var, lo, hi, .. } => {
                        self.walk_expr_chan_dir(lo, env);
                        self.walk_expr_chan_dir(hi, env);
                        if var != "_" {
                            env.define(var, None);
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.walk_expr_chan_dir(expr, env);
                        if matches!(
                            self.chan_dir_of_expr_env(expr, env),
                            Some(hy_syntax::ChanDir::Send)
                        ) {
                            self.emit_range_send_only(expr);
                        }
                        let iter_tys = self.iterate_var_dir_tys(expr, vars.len(), env);
                        for (name, ty) in vars.iter().zip(iter_tys) {
                            if name != "_" {
                                env.define(name, ty);
                            }
                        }
                    }
                    ForKind::Infinite => {}
                }
                for s in &body.stmts {
                    self.walk_stmt_chan_dir(s, env);
                }
                env.pop();
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.walk_expr_chan_dir(v, env);
                }
                let want_tys = env.current_results();
                if !want_tys.is_empty() && !values.is_empty() {
                    if values.len() == 1 && want_tys.len() > 1 {
                        if let Some(got_tys) = self.dir_multi_ty_of_call(&values[0], env) {
                            for (got_ty, want_ty) in got_tys.iter().zip(want_tys.iter()) {
                                self.check_chan_dir_coercion(
                                    got_ty,
                                    want_ty,
                                    values[0].span(),
                                    &values[0].render(),
                                );
                            }
                        }
                    } else {
                        for (v, want_ty) in values.iter().zip(want_tys.iter()) {
                            self.check_expr_against_ty(v, want_ty, env);
                        }
                    }
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_call_arg_chan_dirs(callee, args, env);
                self.walk_expr_chan_dir(callee, env);
                for a in args {
                    self.walk_expr_chan_dir(&a.value, env);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                env.push();
                for s in &body.stmts {
                    self.walk_stmt_chan_dir(s, env);
                }
                env.pop();
            }
            Stmt::Match { subject, arms, .. } => {
                self.walk_expr_chan_dir(subject, env);
                let opt_elem_ty = self
                    .dir_ty_of_expr(subject, env)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| match t {
                        TypeExpr::Option { elem, .. } => Some(*elem),
                        _ => None,
                    });
                for arm in arms {
                    env.push();
                    if let MatchPattern::Path {
                        binding: Some(bname),
                        ..
                    } = &arm.pattern
                    {
                        if bname != "_" {
                            env.define(bname, opt_elem_ty.clone());
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr_chan_dir(g, env);
                    }
                    for s in &arm.body.stmts {
                        self.walk_stmt_chan_dir(s, env);
                    }
                    env.pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    env.push();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            if matches!(
                                self.chan_dir_of_expr_env(chan, env),
                                Some(hy_syntax::ChanDir::Recv)
                            ) {
                                self.emit_send_recv_only(chan, chan.span().to(value.span()));
                            }
                            if let Some(chan_ty) = self.dir_ty_of_expr(chan, env) {
                                if let TypeExpr::Chan {
                                    elem: want_elem, ..
                                } = self.resolve_type_alias(&chan_ty)
                                {
                                    self.check_expr_against_ty(value, &want_elem, env);
                                }
                            }
                            self.walk_expr_chan_dir(chan, env);
                            self.walk_expr_chan_dir(value, env);
                        }
                        SelectArmKind::Recv { names, chan, .. } => {
                            if matches!(
                                self.chan_dir_of_expr_env(chan, env),
                                Some(hy_syntax::ChanDir::Send)
                            ) {
                                self.emit_recv_send_only(chan, chan.span());
                            }
                            self.walk_expr_chan_dir(chan, env);
                            let elem_ty = self
                                .dir_ty_of_expr(chan, env)
                                .map(|t| self.resolve_type_alias(&t))
                                .and_then(|t| match t {
                                    TypeExpr::Chan { elem, .. } => Some(*elem),
                                    _ => None,
                                });
                            if let Some(first) = names.first() {
                                if first != "_" {
                                    env.define(first, elem_ty);
                                }
                            }
                            for n in names.iter().skip(1) {
                                if n != "_" {
                                    env.define(n, None);
                                }
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    for s in &arm.body.stmts {
                        self.walk_stmt_chan_dir(s, env);
                    }
                    env.pop();
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn walk_place_chan_dir(&mut self, place: &hy_syntax::Place, env: &mut ChanDirEnv) {
        use hy_syntax::PlaceKind;
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                if let Some(base_ty) = self.dir_ty_of_place(base, env) {
                    if let hy_syntax::TypeExpr::Map { key, .. } = self.resolve_type_alias(&base_ty)
                    {
                        self.check_expr_against_ty(index, &key, env);
                    }
                }
                self.walk_place_chan_dir(base, env);
                self.walk_expr_chan_dir(index, env);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.walk_place_chan_dir(base, env);
            }
        }
    }

    fn walk_expr_chan_dir(&mut self, expr: &Expr, env: &mut ChanDirEnv) {
        use hy_syntax::InterpPart;
        match expr {
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "close"
                        && !env.is_bound("close")
                        && !self.fns.contains_key("close")
                    {
                        if let Some(first) = args.first() {
                            if matches!(
                                self.chan_dir_of_expr_env(&first.value, env),
                                Some(hy_syntax::ChanDir::Recv)
                            ) {
                                self.emit_close_recv_only(&first.value);
                            }
                        }
                    }
                }
                self.check_call_arg_chan_dirs(callee, args, env);
                self.walk_expr_chan_dir(callee, env);
                for a in args {
                    self.walk_expr_chan_dir(&a.value, env);
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.walk_expr_chan_dir(lhs, env);
                self.walk_expr_chan_dir(rhs, env);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. } => {
                self.walk_expr_chan_dir(operand, env);
            }
            Expr::Receive { chan, span } => {
                if matches!(
                    self.chan_dir_of_expr_env(chan, env),
                    Some(hy_syntax::ChanDir::Send)
                ) {
                    self.emit_recv_send_only(chan, *span);
                }
                self.walk_expr_chan_dir(chan, env);
            }
            Expr::Index { base, index, .. } => {
                if let Some(base_ty) = self.dir_ty_of_expr(base, env) {
                    if let hy_syntax::TypeExpr::Map { key, .. } = self.resolve_type_alias(&base_ty)
                    {
                        self.check_expr_against_ty(index, &key, env);
                    }
                }
                self.walk_expr_chan_dir(base, env);
                self.walk_expr_chan_dir(index, env);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.walk_expr_chan_dir(base, env);
                if let Some(lo) = lo {
                    self.walk_expr_chan_dir(lo, env);
                }
                if let Some(hi) = hi {
                    self.walk_expr_chan_dir(hi, env);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    if let Some(def) = self.struct_fields_for_type(t) {
                        for f in fields {
                            if let Some((_, fty)) = def.iter().find(|(n, _)| n == &f.name) {
                                self.check_expr_against_ty(&f.value, fty, env);
                            }
                        }
                    }
                }
                for f in fields {
                    self.walk_expr_chan_dir(&f.value, env);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(elem_ty) = elem {
                    for i in items {
                        self.check_expr_against_ty(i, elem_ty, env);
                    }
                }
                for i in items {
                    self.walk_expr_chan_dir(i, env);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                for (k, v) in entries {
                    self.check_expr_against_ty(k, key, env);
                    self.check_expr_against_ty(v, value, env);
                    self.walk_expr_chan_dir(k, env);
                    self.walk_expr_chan_dir(v, env);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.walk_expr_chan_dir(e, env);
                    }
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                env.push();
                env.push_results(results.iter().map(|r| r.ty.clone()).collect());
                for p in params {
                    env.define(&p.name, Some(p.ty.clone()));
                }
                for r in results {
                    if let Some(name) = &r.name {
                        env.define(name, Some(r.ty.clone()));
                    }
                }
                for s in &body.stmts {
                    self.walk_stmt_chan_dir(s, env);
                }
                env.pop_results();
                env.pop();
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_call_arg_chan_dirs(
        &mut self,
        callee: &Expr,
        args: &[hy_syntax::CallArg],
        env: &ChanDirEnv,
    ) {
        if let Some(params) = self.dir_callee_params(callee, args, env) {
            if args.len() == 1 && params.len() > 1 && args[0].name.is_none() {
                if let Some(got_tys) = self.dir_multi_ty_of_call(&args[0].value, env) {
                    for (got_ty, param) in got_tys.iter().zip(params.iter()) {
                        self.check_chan_dir_coercion(
                            got_ty,
                            &param.ty,
                            args[0].value.span(),
                            &args[0].value.render(),
                        );
                    }
                }
            } else {
                for (i, arg) in args.iter().enumerate() {
                    let want_param = if let Some(aname) = &arg.name {
                        params
                            .iter()
                            .find(|p| &p.name == aname)
                            .or_else(|| params.get(i))
                    } else {
                        params.get(i)
                    };
                    if let Some(p) = want_param {
                        self.check_expr_against_ty(&arg.value, &p.ty, env);
                    }
                }
            }
            return;
        }
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if !env.is_bound(name)
                && !self.consts.contains_key(name.as_str())
                && !self.fns.contains_key(name.as_str())
            {
                if name == "append" {
                    if let Some(first) = args.first() {
                        if let Some(slice_ty) = self.dir_ty_of_expr(&first.value, env) {
                            let resolved_slice = self.resolve_type_alias(&slice_ty);
                            if let hy_syntax::TypeExpr::Slice { elem, .. } = &resolved_slice {
                                let elem = *elem.clone();
                                for arg in args.iter().skip(1) {
                                    if arg.spread {
                                        self.check_expr_against_ty(
                                            &arg.value,
                                            &resolved_slice,
                                            env,
                                        );
                                    } else {
                                        self.check_expr_against_ty(&arg.value, &elem, env);
                                    }
                                }
                            }
                        }
                    }
                } else if name == "delete" && args.len() >= 2 {
                    if let Some(map_ty) = self.dir_ty_of_expr(&args[0].value, env) {
                        if let hy_syntax::TypeExpr::Map { key, .. } =
                            self.resolve_type_alias(&map_ty)
                        {
                            self.check_expr_against_ty(&args[1].value, &key, env);
                        }
                    }
                } else if args.len() == 1 {
                    if let Some(alias_ty) = self.type_aliases.get(name.as_str()).cloned() {
                        self.check_expr_against_ty(&args[0].value, &alias_ty, env);
                    }
                }
            }
        }
    }

    fn check_expr_against_ty(
        &mut self,
        expr: &Expr,
        want_ty: &hy_syntax::TypeExpr,
        env: &ChanDirEnv,
    ) {
        if let Some(got_ty) = self.dir_ty_of_expr(expr, env) {
            self.check_chan_dir_coercion(&got_ty, want_ty, expr.span(), &expr.render());
        }
        match unwrap_parens(expr) {
            Expr::StructLit {
                ty: None, fields, ..
            } => {
                if let Some(def) = self.struct_fields_for_type(want_ty) {
                    for f in fields {
                        if let Some((_, fty)) = def.iter().find(|(n, _)| n == &f.name) {
                            self.check_expr_against_ty(&f.value, fty, env);
                        }
                    }
                }
            }
            Expr::SliceLit {
                elem: None, items, ..
            } => {
                let resolved = self.resolve_type_alias(want_ty);
                if let hy_syntax::TypeExpr::Slice { elem, .. }
                | hy_syntax::TypeExpr::Array { elem, .. } = resolved
                {
                    for item in items {
                        self.check_expr_against_ty(item, &elem, env);
                    }
                }
            }
            Expr::Call { callee, args, .. } => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "some"
                        && !env.is_bound("some")
                        && !self.fns.contains_key("some")
                    {
                        if let Some(first) = args.first() {
                            if self.dir_ty_of_expr(&first.value, env).is_none() {
                                if let hy_syntax::TypeExpr::Option { elem, .. } =
                                    self.resolve_type_alias(want_ty)
                                {
                                    self.check_expr_against_ty(&first.value, &elem, env);
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn check_chan_dir_coercion(
        &mut self,
        got_ty: &hy_syntax::TypeExpr,
        want_ty: &hy_syntax::TypeExpr,
        span: Span,
        value_desc: &str,
    ) {
        if self.chan_dir_coercion_violation(got_ty, want_ty, true) {
            self.emit_chan_dir_coercion(got_ty, want_ty, span, value_desc);
        }
    }

    fn chan_dir_coercion_violation(
        &self,
        got_ty: &hy_syntax::TypeExpr,
        want_ty: &hy_syntax::TypeExpr,
        top_level: bool,
    ) -> bool {
        use hy_syntax::{ChanDir, TypeExpr};
        let got = self.resolve_type_alias(got_ty);
        let want = self.resolve_type_alias(want_ty);
        match (&got, &want) {
            (
                TypeExpr::Chan {
                    dir: got_dir,
                    elem: got_elem,
                    ..
                },
                TypeExpr::Chan {
                    dir: want_dir,
                    elem: want_elem,
                    ..
                },
            ) => {
                let dir_ok = if top_level {
                    got_dir == want_dir || *got_dir == ChanDir::Both
                } else {
                    got_dir == want_dir
                };
                if !dir_ok {
                    return true;
                }
                self.chan_dir_coercion_violation(got_elem, want_elem, false)
            }
            (TypeExpr::Slice { elem: a, .. }, TypeExpr::Slice { elem: b, .. })
            | (TypeExpr::Array { elem: a, .. }, TypeExpr::Array { elem: b, .. })
            | (TypeExpr::Pointer { to: a, .. }, TypeExpr::Pointer { to: b, .. })
            | (TypeExpr::Option { elem: a, .. }, TypeExpr::Option { elem: b, .. }) => {
                self.chan_dir_coercion_violation(a, b, false)
            }
            (
                TypeExpr::Map {
                    key: ka,
                    value: va,
                    ..
                },
                TypeExpr::Map {
                    key: kb,
                    value: vb,
                    ..
                },
            ) => {
                self.chan_dir_coercion_violation(ka, kb, false)
                    || self.chan_dir_coercion_violation(va, vb, false)
            }
            (
                TypeExpr::Fn {
                    params: pa,
                    results: ra,
                    ..
                },
                TypeExpr::Fn {
                    params: pb,
                    results: rb,
                    ..
                },
            ) => {
                (pa.len() == pb.len()
                    && pa
                        .iter()
                        .zip(pb.iter())
                        .any(|(a, b)| self.chan_dir_coercion_violation(&a.ty, &b.ty, false)))
                    || (ra.len() == rb.len()
                        && ra
                            .iter()
                            .zip(rb.iter())
                            .any(|(a, b)| self.chan_dir_coercion_violation(a, b, false)))
            }
            (TypeExpr::Named { name: na, .. }, TypeExpr::Named { name: nb, .. }) => {
                if let (Some((ba, args_a)), Some((bb, args_b))) = (
                    Self::parse_generic_type_args(na),
                    Self::parse_generic_type_args(nb),
                ) {
                    ba == bb
                        && args_a.len() == args_b.len()
                        && args_a
                            .iter()
                            .zip(args_b.iter())
                            .any(|(a, b)| self.chan_dir_coercion_violation(a, b, false))
                } else {
                    false
                }
            }
            _ => false,
        }
    }

    fn parse_generic_type_args(name: &str) -> Option<(&str, Vec<hy_syntax::TypeExpr>)> {
        let open = name.find('[')?;
        if !name.ends_with(']') || open == 0 {
            return None;
        }
        let base = &name[..open];
        let inner = &name[open + 1..name.len() - 1];
        if inner.trim().is_empty() {
            return None;
        }
        let mut parts = Vec::new();
        let mut depth = 0usize;
        let mut seg_start = 0usize;
        for (idx, ch) in inner.char_indices() {
            match ch {
                '[' | '(' => depth += 1,
                ']' | ')' => depth = depth.saturating_sub(1),
                ',' if depth == 0 => {
                    parts.push(&inner[seg_start..idx]);
                    seg_start = idx + 1;
                }
                _ => {}
            }
        }
        parts.push(&inner[seg_start..]);
        let mut parsed = Vec::with_capacity(parts.len());
        for part in parts {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                return None;
            }
            let snippet = format!("type _t = {}\n", trimmed);
            let file = hy_syntax::parse_file("", &snippet).ok()?;
            match file.decls.into_iter().next()? {
                hy_syntax::Decl::TypeAlias(ta) => parsed.push(ta.ty),
                _ => return None,
            }
        }
        Some((base, parsed))
    }

    fn substitute_type_params(
        ty: &hy_syntax::TypeExpr,
        subst: &std::collections::HashMap<String, hy_syntax::TypeExpr>,
    ) -> hy_syntax::TypeExpr {
        use hy_syntax::TypeExpr;
        if subst.is_empty() {
            return ty.clone();
        }
        match ty {
            TypeExpr::Named { name, span } => {
                if let Some(repl) = subst.get(name) {
                    return repl.clone();
                }
                if let Some((base, args)) = Self::parse_generic_type_args(name) {
                    let joined = args
                        .iter()
                        .map(|a| Self::substitute_type_params(a, subst).render())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return TypeExpr::Named {
                        name: format!("{}[{}]", base, joined),
                        span: *span,
                    };
                }
                ty.clone()
            }
            TypeExpr::Pointer { to, span } => TypeExpr::Pointer {
                to: Box::new(Self::substitute_type_params(to, subst)),
                span: *span,
            },
            TypeExpr::Slice { elem, span } => TypeExpr::Slice {
                elem: Box::new(Self::substitute_type_params(elem, subst)),
                span: *span,
            },
            TypeExpr::Array { len, elem, span } => TypeExpr::Array {
                len: len.clone(),
                elem: Box::new(Self::substitute_type_params(elem, subst)),
                span: *span,
            },
            TypeExpr::Map { key, value, span } => TypeExpr::Map {
                key: Box::new(Self::substitute_type_params(key, subst)),
                value: Box::new(Self::substitute_type_params(value, subst)),
                span: *span,
            },
            TypeExpr::Chan { dir, elem, span } => TypeExpr::Chan {
                dir: *dir,
                elem: Box::new(Self::substitute_type_params(elem, subst)),
                span: *span,
            },
            TypeExpr::Option { elem, span } => TypeExpr::Option {
                elem: Box::new(Self::substitute_type_params(elem, subst)),
                span: *span,
            },
            TypeExpr::Fn {
                params,
                results,
                span,
            } => TypeExpr::Fn {
                params: params
                    .iter()
                    .map(|p| hy_syntax::Param {
                        name: p.name.clone(),
                        ty: Self::substitute_type_params(&p.ty, subst),
                        span: p.span,
                    })
                    .collect(),
                results: results
                    .iter()
                    .map(|r| Self::substitute_type_params(r, subst))
                    .collect(),
                span: *span,
            },
        }
    }

    fn peel_pointer_and_alias(&self, ty: &hy_syntax::TypeExpr) -> hy_syntax::TypeExpr {
        let mut cur = self.resolve_type_alias(ty);
        while let hy_syntax::TypeExpr::Pointer { to, .. } = cur {
            cur = self.resolve_type_alias(&to);
        }
        cur
    }

    fn struct_fields_for_type(
        &self,
        ty: &hy_syntax::TypeExpr,
    ) -> Option<Vec<(String, hy_syntax::TypeExpr)>> {
        let peeled = self.peel_pointer_and_alias(ty);
        let hy_syntax::TypeExpr::Named { name, .. } = &peeled else {
            return None;
        };
        let sname = Self::base_named_type_str(name);
        let fields = self.structs.get(sname)?.clone();
        if let Some((_, type_args)) = Self::parse_generic_type_args(name) {
            if let Some(sdecl) = self.file.decls.iter().find_map(|d| match d {
                hy_syntax::Decl::Struct(s) if s.name == sname => Some(s),
                _ => None,
            }) {
                let mut subst = std::collections::HashMap::new();
                for (tp, arg) in sdecl.type_params.iter().zip(type_args.iter()) {
                    subst.insert(tp.name.clone(), arg.clone());
                }
                if !subst.is_empty() {
                    return Some(
                        fields
                            .into_iter()
                            .map(|(n, fty)| (n, Self::substitute_type_params(&fty, &subst)))
                            .collect(),
                    );
                }
            }
        }
        Some(fields)
    }

    fn infer_type_param_bindings(
        &self,
        param_ty: &hy_syntax::TypeExpr,
        arg_ty: &hy_syntax::TypeExpr,
        tp_names: &std::collections::HashSet<String>,
        out: &mut std::collections::HashMap<String, hy_syntax::TypeExpr>,
    ) {
        use hy_syntax::{ChanDir, TypeExpr};
        if let TypeExpr::Named { name, .. } = param_ty {
            if tp_names.contains(name) {
                match out.entry(name.clone()) {
                    std::collections::hash_map::Entry::Vacant(v) => {
                        v.insert(arg_ty.clone());
                    }
                    std::collections::hash_map::Entry::Occupied(mut o) => {
                        let existing_res = self.resolve_type_alias(o.get());
                        let new_res = self.resolve_type_alias(arg_ty);
                        if matches!(
                            (&existing_res, &new_res),
                            (
                                TypeExpr::Chan { dir: existing_dir, .. },
                                TypeExpr::Chan { dir: ChanDir::Both, .. }
                            ) if *existing_dir != ChanDir::Both
                        ) {
                            o.insert(arg_ty.clone());
                        }
                    }
                }
                return;
            }
        }
        let p_res = self.resolve_type_alias(param_ty);
        let a_res = self.resolve_type_alias(arg_ty);
        match (&p_res, &a_res) {
            (TypeExpr::Chan { elem: pe, .. }, TypeExpr::Chan { elem: ae, .. })
            | (TypeExpr::Slice { elem: pe, .. }, TypeExpr::Slice { elem: ae, .. })
            | (TypeExpr::Array { elem: pe, .. }, TypeExpr::Array { elem: ae, .. })
            | (TypeExpr::Pointer { to: pe, .. }, TypeExpr::Pointer { to: ae, .. })
            | (TypeExpr::Option { elem: pe, .. }, TypeExpr::Option { elem: ae, .. }) => {
                self.infer_type_param_bindings(pe, ae, tp_names, out);
            }
            (
                TypeExpr::Map {
                    key: pk, value: pv, ..
                },
                TypeExpr::Map {
                    key: ak, value: av, ..
                },
            ) => {
                self.infer_type_param_bindings(pk, ak, tp_names, out);
                self.infer_type_param_bindings(pv, av, tp_names, out);
            }
            (TypeExpr::Named { name: pn, .. }, TypeExpr::Named { name: an, .. }) => {
                if let (Some((pb, pargs)), Some((ab, aargs))) = (
                    Self::parse_generic_type_args(pn),
                    Self::parse_generic_type_args(an),
                ) {
                    if pb == ab && pargs.len() == aargs.len() {
                        for (pa, aa) in pargs.iter().zip(aargs.iter()) {
                            self.infer_type_param_bindings(pa, aa, tp_names, out);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn fn_subst_for_call(
        &self,
        f: &hy_syntax::FnDecl,
        callee_name: &str,
        receiver_ty: Option<&hy_syntax::TypeExpr>,
        args: &[hy_syntax::CallArg],
        env: &ChanDirEnv,
        infer_from_args: bool,
    ) -> std::collections::HashMap<String, hy_syntax::TypeExpr> {
        let mut subst = std::collections::HashMap::new();
        if let (Some(rdecl), Some(ract)) = (&f.receiver, receiver_ty) {
            let rdecl_peeled = self.peel_pointer_and_alias(&rdecl.ty);
            let ract_peeled = self.peel_pointer_and_alias(ract);
            if let (
                hy_syntax::TypeExpr::Named { name: dn, .. },
                hy_syntax::TypeExpr::Named { name: an, .. },
            ) = (&rdecl_peeled, &ract_peeled)
            {
                if let (Some((db, dargs)), Some((ab, aargs))) = (
                    Self::parse_generic_type_args(dn),
                    Self::parse_generic_type_args(an),
                ) {
                    if db == ab {
                        for (darg, aarg) in dargs.iter().zip(aargs.iter()) {
                            if let hy_syntax::TypeExpr::Named { name: tp_name, .. } = darg {
                                subst.insert(tp_name.clone(), aarg.clone());
                            }
                        }
                    }
                }
            }
        }
        if let Some((_, explicit_args)) = Self::parse_generic_type_args(callee_name) {
            for (tp, arg) in f.type_params.iter().zip(explicit_args.iter()) {
                subst.insert(tp.name.clone(), arg.clone());
            }
        } else if infer_from_args && !f.type_params.is_empty() {
            let tp_names: std::collections::HashSet<String> =
                f.type_params.iter().map(|tp| tp.name.clone()).collect();
            for (i, arg) in args.iter().enumerate() {
                let param = if let Some(aname) = &arg.name {
                    f.params
                        .iter()
                        .find(|p| &p.name == aname)
                        .or_else(|| f.params.get(i))
                } else {
                    f.params.get(i)
                };
                if let Some(p) = param {
                    if let Some(arg_ty) = self.dir_ty_of_expr(&arg.value, env) {
                        self.infer_type_param_bindings(&p.ty, &arg_ty, &tp_names, &mut subst);
                    }
                }
            }
        }
        subst
    }

    fn emit_chan_dir_coercion(
        &mut self,
        got_ty: &hy_syntax::TypeExpr,
        want_ty: &hy_syntax::TypeExpr,
        span: Span,
        value_desc: &str,
    ) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3012" && d.span == span)
        {
            return;
        }
        let got_str = self.resolve_type_alias(got_ty).render();
        let want_str = self.resolve_type_alias(want_ty).render();
        let msg = format!(
            "cannot use `{}` (`{}`) where `{}` is required — a unidirectional channel never converts to another direction (only `chan T` narrows) (HYB-E3012)",
            value_desc, got_str, want_str
        );
        let notes = vec![
            format!(
                "why: Go permits implicit channel direction conversion only from bidirectional `chan T` to `chan<- T` or `<-chan T`; converting `{}` to `{}` fails `go vet` / `go build` (issue #623)",
                got_str, want_str
            ),
            "fix: declare the source channel as bidirectional (`chan T`), or change the destination's declared channel direction to match the source".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3012", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn iterate_var_dir_tys(
        &self,
        expr: &Expr,
        n_vars: usize,
        env: &ChanDirEnv,
    ) -> Vec<Option<hy_syntax::TypeExpr>> {
        use hy_syntax::TypeExpr;
        let Some(ty) = self.dir_ty_of_expr(expr, env) else {
            return vec![None; n_vars.max(1)];
        };
        match self.resolve_type_alias(&ty) {
            TypeExpr::Chan { elem, .. } => {
                let e = Some(*elem);
                if n_vars >= 2 {
                    vec![None, e]
                } else {
                    vec![e]
                }
            }
            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                let e = Some(*elem);
                if n_vars >= 2 {
                    vec![None, e]
                } else {
                    vec![e]
                }
            }
            TypeExpr::Map { key, value, .. } => {
                if n_vars >= 2 {
                    vec![Some(*key), Some(*value)]
                } else {
                    vec![Some(*value)]
                }
            }
            _ => vec![None; n_vars.max(1)],
        }
    }

    fn resolve_const_dir_ty(&self, name: &str, env: &ChanDirEnv) -> Option<hy_syntax::TypeExpr> {
        let c = self.file.decls.iter().rev().find_map(|d| match d {
            hy_syntax::Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })?;
        if let Some(ty) = &c.ty {
            return Some(ty.clone());
        }
        let mut guard_env = env.clone();
        guard_env.define(name, None);
        self.dir_ty_of_expr(&c.value, &guard_env)
    }

    fn dir_callee_params(
        &self,
        callee: &Expr,
        args: &[hy_syntax::CallArg],
        env: &ChanDirEnv,
    ) -> Option<Vec<hy_syntax::Param>> {
        use hy_syntax::TypeExpr;
        match unwrap_parens(callee) {
            Expr::FnLit { params, .. } => Some(params.clone()),
            Expr::Ident { name, .. } => {
                if let Some(found) = env.lookup(name) {
                    let bty = found?;
                    if let TypeExpr::Fn { params, .. } = self.resolve_type_alias(&bty) {
                        return Some(params);
                    }
                    return None;
                }
                if self.consts.contains_key(name) {
                    if let Some(bty) = self.resolve_const_dir_ty(name, env) {
                        if let TypeExpr::Fn { params, .. } = self.resolve_type_alias(&bty) {
                            return Some(params);
                        }
                    }
                    return None;
                }
                let head = Self::base_named_type_str(name);
                let f = self.fns.get(name.as_str()).or_else(|| self.fns.get(head))?;
                let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                Some(
                    f.params
                        .iter()
                        .map(|p| hy_syntax::Param {
                            name: p.name.clone(),
                            ty: Self::substitute_type_params(&p.ty, &subst),
                            span: p.span,
                        })
                        .collect(),
                )
            }
            Expr::Field { base, name, .. } => {
                let head = Self::base_named_type_str(name);
                if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                    if !env.is_bound(root) {
                        let full = format!("{}.{}", root, head);
                        if let Some(f) = self.fns.get(full.as_str()) {
                            let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                            return Some(
                                f.params
                                    .iter()
                                    .map(|p| hy_syntax::Param {
                                        name: p.name.clone(),
                                        ty: Self::substitute_type_params(&p.ty, &subst),
                                        span: p.span,
                                    })
                                    .collect(),
                            );
                        }
                    }
                }
                let base_ty = self.dir_ty_of_expr(base, env)?;
                let resolved_base = self.resolve_type_alias(&base_ty);
                let recv_base = Self::base_named_type(&resolved_base)
                    .or_else(|| Self::base_named_type(&base_ty))?;
                if recv_base == "set" && matches!(head, "insert" | "contains" | "remove") {
                    let peeled = self.peel_pointer_and_alias(&base_ty);
                    if let TypeExpr::Named { name: sname, .. } = &peeled {
                        if let Some((_, mut targs)) = Self::parse_generic_type_args(sname) {
                            if let Some(elem_ty) = targs.pop() {
                                return Some(vec![hy_syntax::Param {
                                    name: "v".to_string(),
                                    ty: elem_ty,
                                    span: callee.span(),
                                }]);
                            }
                        }
                    }
                }
                if let Some(m) = self
                    .method_decls
                    .get(&(recv_base.clone(), head.to_string()))
                    .or_else(|| {
                        Self::base_named_type(&base_ty)
                            .and_then(|raw| self.method_decls.get(&(raw, head.to_string())))
                    })
                {
                    let subst = self.fn_subst_for_call(m, name, Some(&base_ty), args, env, true);
                    return Some(
                        m.params
                            .iter()
                            .map(|p| hy_syntax::Param {
                                name: p.name.clone(),
                                ty: Self::substitute_type_params(&p.ty, &subst),
                                span: p.span,
                            })
                            .collect(),
                    );
                }
                if let Some(fields) = self.struct_fields_for_type(&base_ty) {
                    if let Some((_, fty)) = fields.iter().find(|(n, _)| n == name) {
                        if let TypeExpr::Fn { params, .. } = self.resolve_type_alias(fty) {
                            return Some(params);
                        }
                    }
                }
                None
            }
            other => {
                let callee_ty = self.dir_ty_of_expr(other, env)?;
                if let TypeExpr::Fn { params, .. } = self.resolve_type_alias(&callee_ty) {
                    Some(params)
                } else {
                    None
                }
            }
        }
    }

    fn dir_multi_ty_of_call(
        &self,
        expr: &Expr,
        env: &ChanDirEnv,
    ) -> Option<Vec<hy_syntax::TypeExpr>> {
        use hy_syntax::TypeExpr;
        match unwrap_parens(expr) {
            Expr::Receive { chan, span } => {
                let chan_ty = self.dir_ty_of_expr(chan, env)?;
                if let TypeExpr::Chan { elem, .. } = self.resolve_type_alias(&chan_ty) {
                    return Some(vec![
                        *elem,
                        TypeExpr::Named {
                            name: "bool".to_string(),
                            span: *span,
                        },
                    ]);
                }
                None
            }
            Expr::Index { base, span, .. } => {
                let base_ty = self.dir_ty_of_expr(base, env)?;
                if let TypeExpr::Map { value, .. } = self.resolve_type_alias(&base_ty) {
                    return Some(vec![
                        *value,
                        TypeExpr::Named {
                            name: "bool".to_string(),
                            span: *span,
                        },
                    ]);
                }
                None
            }
            Expr::Call { callee, args, .. } => match unwrap_parens(callee) {
                Expr::FnLit { results, .. } => Some(results.iter().map(|r| r.ty.clone()).collect()),
                Expr::Ident { name, .. } => {
                    if let Some(found) = env.lookup(name) {
                        let bty = found?;
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&bty) {
                            return Some(results);
                        }
                        return None;
                    }
                    if self.consts.contains_key(name) {
                        let bty = self.resolve_const_dir_ty(name, env)?;
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&bty) {
                            return Some(results);
                        }
                        return None;
                    }
                    let head = Self::base_named_type_str(name);
                    let f = self.fns.get(name.as_str()).or_else(|| self.fns.get(head))?;
                    let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                    Some(
                        f.results
                            .iter()
                            .map(|r| Self::substitute_type_params(&r.ty, &subst))
                            .collect(),
                    )
                }
                Expr::Field { base, name, .. } => {
                    let head = Self::base_named_type_str(name);
                    if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                        if !env.is_bound(root) {
                            let full = format!("{}.{}", root, head);
                            if let Some(f) = self.fns.get(full.as_str()) {
                                let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                                return Some(
                                    f.results
                                        .iter()
                                        .map(|r| Self::substitute_type_params(&r.ty, &subst))
                                        .collect(),
                                );
                            }
                        }
                    }
                    let base_ty = self.dir_ty_of_expr(base, env)?;
                    let recv_base = Self::base_named_type(&self.resolve_type_alias(&base_ty))
                        .or_else(|| Self::base_named_type(&base_ty))?;
                    let m = self
                        .method_decls
                        .get(&(recv_base, head.to_string()))
                        .or_else(|| {
                            Self::base_named_type(&base_ty)
                                .and_then(|raw| self.method_decls.get(&(raw, head.to_string())))
                        })?;
                    let subst = self.fn_subst_for_call(m, name, Some(&base_ty), args, env, true);
                    Some(
                        m.results
                            .iter()
                            .map(|r| Self::substitute_type_params(&r.ty, &subst))
                            .collect(),
                    )
                }
                other => {
                    let callee_ty = self.dir_ty_of_expr(other, env)?;
                    if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                        Some(results)
                    } else {
                        None
                    }
                }
            },
            _ => None,
        }
    }

    fn dir_ty_of_place(
        &self,
        place: &hy_syntax::Place,
        env: &ChanDirEnv,
    ) -> Option<hy_syntax::TypeExpr> {
        use hy_syntax::{PlaceKind, TypeExpr};
        match &place.kind {
            PlaceKind::Ident(name) => {
                if name == "_" {
                    return None;
                }
                if let Some(found) = env.lookup(name) {
                    return found;
                }
                self.resolve_const_dir_ty(name, env)
            }
            PlaceKind::Field { base, name } => {
                let base_ty = self.dir_ty_of_place(base, env)?;
                self.struct_fields_for_type(&base_ty)
                    .and_then(|fs| fs.into_iter().find(|(n, _)| n == name))
                    .map(|(_, t)| t)
            }
            PlaceKind::Index { base, .. } => {
                let base_ty = self.dir_ty_of_place(base, env)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. }
                    | TypeExpr::Array { elem, .. }
                    | TypeExpr::Map { value: elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            PlaceKind::AddrOf { base } => self.dir_ty_of_place(base, env).map(|t| {
                TypeExpr::Pointer {
                    to: Box::new(t),
                    span: place.span,
                }
            }),
        }
    }

    fn dir_ty_of_expr(&self, expr: &Expr, env: &ChanDirEnv) -> Option<hy_syntax::TypeExpr> {
        use hy_syntax::TypeExpr;
        match expr {
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } => self.dir_ty_of_expr(inner, env),
            Expr::Ident { name, span } => {
                if let Some(found) = env.lookup(name) {
                    return found;
                }
                if self.consts.contains_key(name) {
                    return self.resolve_const_dir_ty(name, env);
                }
                let head = Self::base_named_type_str(name);
                if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                    return Some(TypeExpr::Fn {
                        params: decl.params.clone(),
                        results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                        span: *span,
                    });
                }
                None
            }
            Expr::Field { base, name, .. } => {
                let base_ty = self.dir_ty_of_expr(base, env)?;
                self.struct_fields_for_type(&base_ty)
                    .and_then(|fs| fs.into_iter().find(|(n, _)| n == name))
                    .map(|(_, t)| t)
            }
            Expr::Index { base, .. } => {
                let base_ty = self.dir_ty_of_expr(base, env)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. }
                    | TypeExpr::Array { elem, .. }
                    | TypeExpr::Map { value: elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::Slice { base, span, .. } => {
                let base_ty = self.dir_ty_of_expr(base, env)?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                        Some(TypeExpr::Slice { elem, span: *span })
                    }
                    _ => None,
                }
            }
            Expr::Receive { chan, .. } => {
                let chan_ty = self.dir_ty_of_expr(chan, env)?;
                match self.resolve_type_alias(&chan_ty) {
                    TypeExpr::Chan { elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::StructLit { ty, .. } => ty.clone(),
            Expr::SliceLit {
                elem: Some(el),
                items,
                span,
            } => {
                if let Some((len_opt, inner_elem, arr_ty)) = expr.as_array_lit() {
                    if len_opt.is_some() {
                        return Some(arr_ty.clone());
                    }
                    return Some(TypeExpr::Array {
                        len: Some(Box::new(Expr::Lit {
                            kind: hy_syntax::LitKind::Int(items.len().to_string()),
                            span: *span,
                        })),
                        elem: Box::new(inner_elem.clone()),
                        span: *span,
                    });
                }
                Some(TypeExpr::Slice {
                    elem: el.clone(),
                    span: *span,
                })
            }
            Expr::MapLit {
                key, value, span, ..
            } => Some(TypeExpr::Map {
                key: key.clone(),
                value: value.clone(),
                span: *span,
            }),
            Expr::FnLit {
                params,
                results,
                span,
                ..
            } => Some(TypeExpr::Fn {
                params: params.clone(),
                results: results.iter().map(|r| r.ty.clone()).collect(),
                span: *span,
            }),
            Expr::AddrOf { operand, span }
            | Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                span,
            } => self.dir_ty_of_expr(operand, env).map(|t| {
                TypeExpr::Pointer {
                    to: Box::new(t),
                    span: *span,
                }
            }),
            Expr::Call { callee, args, span } => match unwrap_parens(callee) {
                Expr::FnLit { results, .. } => results.first().map(|r| r.ty.clone()),
                Expr::Ident { name, .. } => {
                    if let Some(bty) = env.lookup(name) {
                        if let Some(bty) = bty {
                            if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&bty) {
                                return results.first().cloned();
                            }
                        }
                        return None;
                    }
                    if self.consts.contains_key(name) {
                        if let Some(bty) = self.resolve_const_dir_ty(name, env) {
                            if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&bty) {
                                return results.first().cloned();
                            }
                        }
                        return None;
                    }
                    if name == "some" && !self.fns.contains_key("some") {
                        let elem = args
                            .first()
                            .and_then(|a| self.dir_ty_of_expr(&a.value, env))?;
                        return Some(TypeExpr::Option {
                            elem: Box::new(elem),
                            span: *span,
                        });
                    }
                    if name == "append" && !self.fns.contains_key("append") {
                        return args
                            .first()
                            .and_then(|a| self.dir_ty_of_expr(&a.value, env));
                    }
                    if name == "make" {
                        return args.first().and_then(|a| match unwrap_parens(&a.value) {
                            Expr::StructLit { ty, .. } => ty.clone(),
                            Expr::MapLit {
                                key, value, span, ..
                            } => Some(TypeExpr::Map {
                                key: key.clone(),
                                value: value.clone(),
                                span: *span,
                            }),
                            Expr::SliceLit {
                                elem: Some(e),
                                span,
                                ..
                            } => Some(TypeExpr::Slice {
                                elem: e.clone(),
                                span: *span,
                            }),
                            Expr::Ident { name: tname, span }
                                if self.type_aliases.contains_key(tname) =>
                            {
                                Some(TypeExpr::Named {
                                    name: tname.clone(),
                                    span: *span,
                                })
                            }
                            _ => None,
                        });
                    }
                    let head = Self::base_named_type_str(name);
                    if let Some(f) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                        let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                        return f
                            .results
                            .first()
                            .map(|r| Self::substitute_type_params(&r.ty, &subst));
                    }
                    if args.len() == 1 && self.type_aliases.contains_key(name.as_str()) {
                        return Some(TypeExpr::Named {
                            name: name.clone(),
                            span: *span,
                        });
                    }
                    None
                }
                Expr::Field { base, name, .. } => {
                    let head = Self::base_named_type_str(name);
                    if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                        if !env.is_bound(root) {
                            let full = format!("{}.{}", root, head);
                            if let Some(f) = self.fns.get(full.as_str()) {
                                let subst = self.fn_subst_for_call(f, name, None, args, env, true);
                                return f
                                    .results
                                    .first()
                                    .map(|r| Self::substitute_type_params(&r.ty, &subst));
                            }
                        }
                    }
                    let base_ty = self.dir_ty_of_expr(base, env)?;
                    let resolved_base = self.resolve_type_alias(&base_ty);
                    if head == "pop" && args.is_empty() {
                        if let TypeExpr::Slice { elem, .. } = resolved_base {
                            return Some(*elem);
                        }
                    }
                    let recv_base = Self::base_named_type(&resolved_base)
                        .or_else(|| Self::base_named_type(&base_ty))?;
                    if let Some(m) = self
                        .method_decls
                        .get(&(recv_base.clone(), head.to_string()))
                        .or_else(|| {
                            Self::base_named_type(&base_ty)
                                .and_then(|raw| self.method_decls.get(&(raw, head.to_string())))
                        })
                    {
                        let subst =
                            self.fn_subst_for_call(m, name, Some(&base_ty), args, env, true);
                        return m
                            .results
                            .first()
                            .map(|r| Self::substitute_type_params(&r.ty, &subst));
                    }
                    if let Some(fields) = self.struct_fields_for_type(&base_ty) {
                        if let Some((_, fty)) = fields.iter().find(|(n, _)| n == name) {
                            if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(fty) {
                                return results.first().cloned();
                            }
                        }
                    }
                    None
                }
                other => {
                    let callee_ty = self.dir_ty_of_expr(other, env)?;
                    if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                        results.first().cloned()
                    } else {
                        None
                    }
                }
            },
            _ => None,
        }
    }

    fn chan_dir_of_expr_env(&self, expr: &Expr, env: &ChanDirEnv) -> Option<hy_syntax::ChanDir> {
        let ty = self.dir_ty_of_expr(expr, env)?;
        match self.resolve_type_alias(&ty) {
            hy_syntax::TypeExpr::Chan { dir, .. } => Some(dir),
            _ => None,
        }
    }

    fn emit_send_recv_only(&mut self, chan: &Expr, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3008" && d.span == span)
        {
            return;
        }
        let msg = format!(
            "cannot send to `{}` — it is a receive-only channel (`<-chan T`)",
            chan.render()
        );
        let notes = vec![
            "why: a `<-chan T` binding only permits receives; sending to it is invalid Go and the emitted program will not build (issue #89)".to_string(),
            "fix: declare the channel as `chan T` (bidirectional) or `chan<- T` (send-only) where the send happens".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3008", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn emit_recv_send_only(&mut self, chan: &Expr, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3008" && d.span == span)
        {
            return;
        }
        let msg = format!(
            "cannot receive from `{}` — it is a send-only channel (`chan<- T`)",
            chan.render()
        );
        let notes = vec![
            "why: a `chan<- T` binding only permits sends; receiving from it is invalid Go and the emitted program will not build (issue #89)".to_string(),
            "fix: declare the channel as `chan T` (bidirectional) or `<-chan T` (receive-only) where the receive happens".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3008", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn emit_close_recv_only(&mut self, chan: &Expr) {
        let span = chan.span();
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3009" && d.span == span)
        {
            return;
        }
        let msg = format!(
            "cannot close `{}` — it is a receive-only channel (`<-chan T`)",
            chan.render()
        );
        let notes = vec![
            "why: the Go spec requires close to operate on a bidirectional or send-only channel — a `<-chan T` has no send capability, so closing it is invalid Go and the emitted program will not build (issue #94)".to_string(),
            "fix: declare this channel as `chan T` (bidirectional) where the close happens, or pass a `chan<- T` / `chan T` binding to close — receive-only channels are closed by their send-side owner".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3009", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    fn emit_range_send_only(&mut self, chan: &Expr) {
        let span = chan.span();
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E3009" && d.span == span)
        {
            return;
        }
        let msg = format!(
            "cannot range over `{}` — it is a send-only channel (`chan<- T`)",
            chan.render()
        );
        let notes = vec![
            "why: the Go spec requires the range operand to have receive capability — a `chan<- T` can only be sent to, so ranging over it is invalid Go and the emitted program will not build (issue #94)".to_string(),
            "fix: declare this channel as `chan T` (bidirectional) or `<-chan T` (receive-only) where the range happens — send-only channels are consumed via receives on the other side".to_string(),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E3009", self.path.clone(), span, msg).with_notes(notes),
        );
    }
}

#[derive(Clone)]
struct ChanDirEnv {
    scopes: Vec<std::collections::HashMap<String, Option<hy_syntax::TypeExpr>>>,
    return_stack: Vec<Vec<hy_syntax::TypeExpr>>,
}

impl ChanDirEnv {
    fn new() -> Self {
        ChanDirEnv {
            scopes: vec![std::collections::HashMap::new()],
            return_stack: Vec::new(),
        }
    }

    fn push(&mut self) {
        self.scopes.push(std::collections::HashMap::new());
    }

    fn pop(&mut self) {
        if self.scopes.len() > 1 {
            self.scopes.pop();
        }
    }

    fn push_results(&mut self, results: Vec<hy_syntax::TypeExpr>) {
        self.return_stack.push(results);
    }

    fn pop_results(&mut self) {
        self.return_stack.pop();
    }

    fn current_results(&self) -> Vec<hy_syntax::TypeExpr> {
        self.return_stack.last().cloned().unwrap_or_default()
    }

    fn define(&mut self, name: &str, ty: Option<hy_syntax::TypeExpr>) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), ty);
        }
    }

    fn lookup_current_scope(&self, name: &str) -> Option<Option<hy_syntax::TypeExpr>> {
        self.scopes.last().and_then(|top| top.get(name).cloned())
    }

    fn lookup(&self, name: &str) -> Option<Option<hy_syntax::TypeExpr>> {
        for layer in self.scopes.iter().rev() {
            if let Some(found) = layer.get(name) {
                return Some(found.clone());
            }
        }
        None
    }

    fn is_bound(&self, name: &str) -> bool {
        self.lookup(name).is_some()
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


