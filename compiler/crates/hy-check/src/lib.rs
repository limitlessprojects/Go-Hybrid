//! hy-check — the Tier-0 checker (SPEC-LANGUAGE.md §9).
//!
//! Public API (consumed by the `hyb` CLI and the corpus harness):
//!   * `check_file(&File) -> CheckResult` — diagnostics + verdict + managed report
//!
//! Architecture (two passes per function, one flat file pass):
//!   * `model.rs`   — the data model: TypeShape classification, bindings,
//!     moves/consumes, loans, the Checker accumulator.
//!   * `walk.rs`    — Pass A: flattens a body into a source-ordered statement
//!     list with indices; records loop/managed extents (the
//!     non-lexical windows) and per-binding last-use indices;
//!     collects per-statement uses and calls (closures are
//!     skipped there — the A5 perimeter; closure BOUNDARY
//!     statements are checked by closures.rs).
//!   * `closures.rs` — the closure-boundary pass: task/send/select-send
//!     statements inside closure bodies are still concurrency
//!     boundaries in the emitted Go — checked fail-closed (O4.9),
//!     while captures/loans/freeze stay outside the checked
//!     subset (A5, documented).
//!   * `shape.rs`   — lightweight shape inference from declared types and
//!     initializer expressions (NOT a type system).
//!   * `foreign.rs`— the builtin foreign-type + method-mode descriptor
//!     stubs (documented tech debt: replaced by generated
//!     boundary descriptors post-MVP).
//!   * `sendability.rs` — A1c (O4 classes) at spawn/send sites + the O6
//!     `managed` suppression/lint (HYB-E8001).
//!   * `freeze.rs`  — A1b/A7 (O5 freeze-after-move, O13 consume).
//!   * `loans.rs`   — A2/A3/A6/LOAN-ESCAPE + the environment updates.
//!   * `const_overflow.rs` — HYB-E6002: constant init outside a fixed-width declared type
//!     (issue #148; #201-B extends the fold to constant expressions and float→int).
//!   * `emit_guard.rs` — HYB-E2006/E1003/E2007/E6005/E6004: expression and
//!     declaration guards that keep accepted programs emitting buildable Go
//!     (issue #201: incomparable `==`, package-level redeclarations,
//!     invalid receivers, negative shift counts, bad array lengths).
//!   * this file    — Pass B: the per-statement driver and the verdict.
//!   * `builtins.rs` — HYB-E2001 defer-non-call + HYB-E2005 builtin args (issue #187).
//!
//! Tier-0 discipline (O4.9, rule FC — human directive 2026-10-13): the
//! checker is FAIL-CLOSED at spawn/send boundaries — a value whose type it
//! cannot resolve is UNPROVEN, never silently safe (HYB-E3005); the escape
//! is the explicit, auditable `managed` hatch (HYB-E8001). Inside a
//! function, unknown locals remain legal (no whole-program type system);
//! every rejection teaches (O17 — fix-its come from the hy-diag registry);
//! diagnostics are returned in source order.

mod addr_loan;
mod arith;
mod buildable;
mod const_overflow;
mod closures;
mod emit_guard;
mod foreign;
mod freeze;
mod loans;
mod const_init;
mod model;
mod resolve;
mod sendability;
mod shape;
mod tuple;
mod walk;
mod builtins;
mod range_iter;
mod recv;

use hy_diag::{Diagnostic, ManagedBlockAudit, ManagedReport, Severity};

use hy_syntax::{Decl, Expr, File, FnDecl, ForKind, SelectArmKind, Stmt};
use model::{Binding, Checker, FnState, MANAGED_CATEGORIES, TypeShape};

/// The result of checking one file.
#[derive(Debug, Default)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    /// `accept` when no errors remain (warnings allowed), else `reject`.
    pub verdict: Verdict,
    pub managed: ManagedReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verdict {
    #[default]
    Accept,
    Reject,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Accept => "accept",
            Verdict::Reject => "reject",
        }
    }
}

/// Check one parsed file against the Tier-0 rules (O2/O4/O5/O6/O7/O8/O12...).
///
/// Verdict = reject iff any severity-error diagnostic survives (warnings —
/// including the managed lint — never fail a build). Diagnostics are sorted
/// into source order before returning: the corpus harness reads the FIRST
/// error's `tier0_rule` as the join key against VERDICTS.md.
pub fn check_file(file: &File) -> CheckResult {
    let mut ck = Checker::new(file);

    build_tables(&mut ck);
    ck.check_struct_decls(); // A6: borrow-handle / pointer struct fields
    ck.check_file_const_zero_divisors(); // Issue #149/#508: constant zero divisor in file consts

    // Issue #228 family A: value-position names nothing declares must be
    // HYB-E1001 here, not `undefined: X` at go build. Const initializers and
    // declaration-signature expressions (array lengths) at file level; each
    // function body inside check_fn below.
    ck.check_file_undefined_names();

    let fns: Vec<&FnDecl> = file.fns().collect();
    for f in fns {
        check_fn(&mut ck, f);
    }

    // Issue #134: an accepted program must emit buildable Go — unused `:=`
    // bindings (incl. `_ := 5`), unused imports, and repeated names on the
    // left of one `:=` are rejected fail-closed as HYB-E1002.
    buildable::check_buildable(&mut ck);

    let mut diags = ck.diags;
    diags.sort_by(|a, b| {
        (
            a.span.start.line,
            a.span.start.col,
            a.span.end.line,
            a.span.end.col,
        )
            .cmp(&(
                b.span.start.line,
                b.span.start.col,
                b.span.end.line,
                b.span.end.col,
            ))
    });
    let verdict = if diags.iter().any(|d| d.severity == Severity::Error) {
        Verdict::Reject
    } else {
        Verdict::Accept
    };
    CheckResult {
        diagnostics: diags,
        verdict,
        managed: ManagedReport {
            managed_blocks: ck.managed_blocks,
            sites_via_managed: ck.sites_via_managed,
            total_sites: ck.total_sites,
            categories: {
                let mut m = std::collections::BTreeMap::new();
                for b in &ck.managed_audit {
                    *m.entry(b.category.clone()).or_insert(0usize) += 1;
                }
                m
            },
            blocks: ck.managed_audit.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// File-level tables
// ---------------------------------------------------------------------------

fn build_tables(ck: &mut Checker<'_>) {
    let file: &File = ck.file;
    for d in &file.decls {
        match d {
            Decl::Import { path, .. } => {
                // `import "rust/serde_json"` → package root "serde_json".
                let root = path.rsplit('/').next().unwrap_or(path.as_str());
                ck.packages.insert(root.to_string());
            }
            Decl::Const(c) => {
                // Issue #148: a file-level `const n int8 = 300` must not
                // pass the checker and then fail `go build` (overflow).
                if let Some(ty) = &c.ty {
                    ck.check_const_overflow_init(&c.name, ty, &c.value, None);
                }
                ck.declare_pkg_name(&c.name, "const", c.span);
                let b = ck.const_binding(c);
                ck.consts.insert(c.name.clone(), b);
                ck.const_values.insert(c.name.clone(), &c.value);
                // Issue #205: file-level `const c = get_val()` must not pass check then fail go build.
                // Placed after insert so this hook sits below main's E6002 overflow check and
                // 3-way-merges cleanly (same pattern as PR #191 relocated hooks).
                ck.check_const_init_is_constant(&c.name, &c.value, None);
            }
            Decl::Struct(s) => {
                // Issue #201-C: package-scope names are unique across
                // fns / consts / structs (Go's single package block).
                ck.declare_pkg_name(&s.name, "struct", s.span);
                // `type X = int` aliases parse as zero-field struct decls;
                // they are not containers — leave them unresolved (Unknown
                // is permissive, and alias-of-primitive values must never
                // classify as mutable GC structs).
                if s.fields.is_empty() {
                    continue;
                }
                // Issue #201-C: duplicate field names in one struct are
                // `a redeclared in this block` in the emitted Go.
                let mut seen_fields: std::collections::HashSet<&str> =
                    std::collections::HashSet::new();
                for f in &s.fields {
                    if !seen_fields.insert(f.name.as_str()) {
                        ck.diags.push(
                            Diagnostic::error(
                                "HYB-E1003",
                                ck.path.clone(),
                                f.span,
                                format!(
                                    "field `{}` redeclared in struct `{}` (HYB-E1003)",
                                    f.name, s.name
                                ),
                            )
                            .with_notes(vec![
                                format!(
                                    "why: Go rejects a struct with two `{}` fields (`{} redeclared in this block`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                                    f.name, f.name
                                ),
                                format!(
                                    "fix: rename one of the `{}` fields — each struct field name must be unique",
                                    f.name
                                ),
                            ]),
                        );
                    }
                }
                ck.structs.insert(
                    s.name.clone(),
                    s.fields
                        .iter()
                        .map(|f| (f.name.clone(), f.ty.clone()))
                        .collect(),
                );
            }
            Decl::Fn(f) => {
                // Only top-level functions belong in `ck.fns` (methods have a
                // receiver and must not overwrite or impersonate top-level fns).
                if let Some(r) = &f.receiver {
                    if let Some(base) = Checker::base_named_type(&r.ty) {
                        ck.method_decls.insert((base, f.name.clone()), f);
                    }
                    // Issue #201-C/D: method-set dup + receiver validity.
                    ck.declare_method(f);
                } else {
                    ck.declare_pkg_name(&f.name, "fn", f.span);
                    ck.fns.insert(f.name.clone(), f);
                }
            }
            Decl::Enum(e) => {
                ck.enum_has_zero_value.insert(
                    e.name.clone(),
                    e.variants.first().is_some_and(|v| v.payload.is_none()),
                );
                ck.enums.insert(
                    e.name.clone(),
                    e.variants.iter().map(|v| v.name.clone()).collect(),
                );
                ck.declare_pkg_name(&e.name, "enum", e.span);
            }
            Decl::TypeAlias(ta) => {
                ck.type_aliases.insert(ta.name.clone(), ta.ty.clone());
                ck.declare_pkg_name(&ta.name, "type", ta.span);
            }
        }
    }

    // Issue #201-E pass 2: array-length validity over every DECLARED type,
    // after all file-level consts are known (`[n]int` where `n` is a const).
    for d in &file.decls {
        match d {
            Decl::Const(c) => {
                if let Some(ty) = &c.ty {
                    ck.check_type_expr(ty);
                }
                let b = ck.const_binding(c);
                ck.consts.insert(c.name.clone(), b);
            }
            Decl::Struct(s) => {
                for f in &s.fields {
                    ck.check_type_expr(&f.ty);
                }
            }
            Decl::Fn(f) => {
                if let Some(r) = &f.receiver {
                    ck.check_type_expr(&r.ty);
                }
                for p in &f.params {
                    ck.check_type_expr(&p.ty);
                }
                for r in &f.results {
                    ck.check_type_expr(&r.ty);
                }
            }
            Decl::TypeAlias(a) => ck.check_type_expr(&a.ty),
            Decl::Enum(e) => {
                for v in &e.variants {
                    if let Some(p) = &v.payload {
                        ck.check_type_expr(p);
                    }
                }
            }
            Decl::Import { .. } => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Pass B — the per-function statement driver
// ---------------------------------------------------------------------------

fn check_fn(ck: &mut Checker<'_>, f: &FnDecl) {
    let Some(body) = &f.body else { return };

    // Issue #228 family A: `undefined_fn()`, reads/assigns of names nothing
    // declares — HYB-E1001 before emit ever runs. Dotted package-prefix
    // first segments stay exempt (resolve.rs Field arm).
    ck.check_fn_undefined_names(f);

    // environment seed: receiver + params
    let mut st = FnState::default();
    if let Some(r) = &f.receiver {
        let base_shape = ck.shape_of_type(&r.ty);
        let (shape, ty) = if r.is_pointer {
            (
                TypeShape::Pointer(Box::new(base_shape)),
                Some(hy_syntax::TypeExpr::Pointer {
                    to: Box::new(r.ty.clone()),
                    span: r.ty.span(),
                }),
            )
        } else {
            (base_shape, Some(r.ty.clone()))
        };
        st.env.insert(
            r.name.clone(),
            Binding {
                shape,
                ty,
                immutable: false,
                managed_taint: false,
                moved: None,
                non_owned_origin: Some(format!(
                    "method receiver `{}` (the caller retains the receiver across intra-task calls)",
                    r.name
                )),
                slot_loan: false,
            },
        );
    }
    for p in &f.params {
        let shape = ck.shape_of_type(&p.ty);
        st.env.insert(
            p.name.clone(),
            Binding {
                shape,
                ty: Some(p.ty.clone()),
                immutable: false,
                managed_taint: false,
                moved: None,
                non_owned_origin: Some(format!(
                    "parameter `{}` (the caller retains the argument across intra-task calls)",
                    p.name
                )),
                slot_loan: false,
            },
        );
    }

    // N1: the declared result shapes (return-site nil checks, HYB-E4001),
    // plus named result bindings (`fn f() (out map[string]int)`), which are
    // function-scoped mutable local variables in Go and returned by bare `return`.
    for r in &f.results {
        let shape = ck.shape_of_type(&r.ty);
        if let Some(name) = &r.name {
            if name != "_" {
                st.env.insert(
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
                st.named_results.push(name.clone());
            }
        }
        st.result_shapes.push(shape);
        st.result_types.push(r.ty.clone());
    }

    // Pass A: indices, structural extents, last-use map.
    let flat = walk::flatten_with_results(body, &st.named_results);
    st.last_use = flat.last_use.clone();

    let mut active_loops: Vec<(
        usize,
        usize,
        usize,
        &hy_syntax::Block,
        std::collections::HashSet<String>,
    )> = Vec::new();
    let mut reported_backedge: std::collections::HashSet<String> =
        std::collections::HashSet::new();

    // Pass B: statements in source order.
    for (stmt, idx) in &flat.stmts {
        let stmt: &Stmt = stmt;
        let idx = *idx;
        let managed = flat.in_managed(idx);
        // Issue #14: apply the statement's scope events BEFORE processing, in
        // exact order so multiple Pop/Push events at the same index never
        // coalesce into one, and select-receive arm bindings live inside their
        // own arm scope.
        if let Some(evs) = flat.events.get(&idx) {
            for ev in evs {
                match ev {
                    walk::ScopeEvent::Push => st.scope_push(),
                    walk::ScopeEvent::Pop => st.scope_pop(),
                    walk::ScopeEvent::SelectRecv { names, chan } => {
                        let elem = match ck.shape_of_expr(&st, chan) {
                            TypeShape::Chan(e) => *e,
                            _ => TypeShape::Unknown,
                        };
                        for (i, n) in names.iter().enumerate() {
                            let shape = if i == 0 {
                                elem.clone()
                            } else {
                                TypeShape::Primitive
                            };
                            ck.define_binding(&mut st, n, shape, None, false, managed);
                        }
                    }
                    walk::ScopeEvent::MatchBind { name, subject } => {
                        ck.bind_match_arm(&mut st, name, subject, managed);
                    }
                }
            }
        }
        if let Stmt::For { body: lbody, .. } = stmt {
            let outer_depth = st.scopes.len().saturating_sub(1);
            let pre_moved = st
                .env
                .iter()
                .chain(st.scopes.iter().flat_map(|s| s.iter()))
                .filter(|(_, b)| b.moved.is_some())
                .map(|(k, _)| k.clone())
                .collect();
            active_loops.push((idx, flat.loop_end_of(idx), outer_depth, lbody, pre_moved));
        }
        if let Stmt::Managed {
            reason,
            category,
            tickets,
            span,
            ..
        } = stmt
        {
            ck.audit_managed_block(reason, category, tickets, *span);
        }

        // LX14: `break loop` / `continue loop` bind to the nearest enclosing
        // for/loop. With no enclosing loop in this function body the label
        // has no target — the emitter would lower it to `break hyLoop`
        // against an undefined Go label (`invalid break label hyLoop`).
        // Reject at check time (HYB-E3010); closure bodies are outside the
        // Tier-0 flat walk and are guarded at emit time instead.
        if let Stmt::Break { label: Some(_), span, .. }
        | Stmt::Continue { label: Some(_), span, .. } = stmt
        {
            if !flat.in_loop(idx) {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E3010",
                        ck.path.clone(),
                        *span,
                        "`break loop` / `continue loop` with no enclosing loop — the label has no target here (HYB-E3010)",
                    )
                    .with_notes(vec![
                        "the `loop` label binds to the nearest enclosing `for`/`loop` (LX14); labels do not cross function or closure boundaries".to_string(),
                        "remove the `loop` label (plain `break`/`continue` targets the innermost loop), or move this statement inside a loop".to_string(),
                    ]),
                );
            }
        }

        // A1b/A7: use-after-move on this statement's own uses (checked
        // BEFORE this statement marks any new transfers).
        let mut uses = Vec::new();
        walk::own_uses_with_results(stmt, &st.named_results, &mut uses);
        ck.check_uses(&st, &uses);
        // T24: no address-of on a map-slot loan (HYB-E3104).
        ck.check_addr_of_loan(&st, &uses);

        // Issue #149: `/` or `%` by a literal 0 must not be accepted —
        // Go refuses to compile the emitted expression.
        ck.check_stmt_const_zero_divisor(stmt);

        // Issue #201: expression/type guards (incomparable `==`,
        // constant negative shifts, local array-length ascriptions).
        ck.check_stmt_emit_guard(&st, stmt);
        // T23: a multi-value call bound to the wrong number of names
        // (`p := minmax(3, 9)`) — Go refuses the emitted binding.
        ck.check_stmt_multi_value(&st, stmt);
        // Issue #228 family C: literal misuse in `Type(const)` conversion
        // calls (`int8(300)` on a 64-bit target, `int("abc")`, `string(2.5)`,
        // `[]int(3)`, …) is accepted today and then fails `go build`.
        ck.check_stmt_type_conversions(stmt);

        // Calls (A2 loan windows, A7 consumes + arg lending). A for-loop
        // ITERATE expression's borrow calls get loop-extent loans: the
        // element views live across the whole loop body (O7).
        let mut stmt_consumes = Vec::new();
        if let Stmt::For {
            kind: ForKind::Iterate { expr, .. },
            ..
        } = stmt
        {
            let loop_end = flat.loop_end_of(idx);
            let mut calls = Vec::new();
            walk::expr_calls(expr, &mut calls);
            for cr in calls {
                ck.process_call(&mut st, &cr, idx, Some(loop_end), &mut stmt_consumes);
            }
        } else {
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            for cr in calls {
                ck.process_call(&mut st, &cr, idx, None, &mut stmt_consumes);
            }
        }
        if !stmt_consumes.is_empty() {
            ck.check_same_stmt_consumes(&st, &uses, &stmt_consumes);
        }

        // Statement-specific processing.
        // Issue #187: defer must be a call (HYB-E2001); literal/arity
        // misuse of make/len/cap/delete/close/new (HYB-E2005).
        // After the call/loan walk so it does not collide with upstream
        // inserts around check_uses (const-zero-divisor / LX14).
        ck.check_stmt_builtins(stmt);

        // Issue #411: the iterable of `for x in e` must resolve to a
        // slice/array/string/map/chan (LX13) — HYB-E2009; fail-closed
        // E3005 (O4.9). Same accepted-should-emit-buildable-Go family as
        // the send-target (#77) classification.
        ck.check_stmt_range_iterable(&st, stmt);
        // Issue #409: every receive operand must resolve to a channel
        // (HYB-E2008; fail-closed E3005) — the mirror of the send-side
        // target classification (#77, HYB-E3007).
        ck.check_stmt_recv_operands(&st, stmt);

        match stmt {
            Stmt::Local(l) => {
                ck.process_local(&mut st, l, idx, managed);
                ck.record_local_borrow_aliases(&mut st, l);
                ck.mark_slot_loans_local(&mut st, l);
            }

            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                ck.process_assign(&mut st, targets, *op, values, *span, idx);
                ck.record_assign_borrow_aliases(&mut st, targets, *op, values);
                ck.mark_slot_loans_assign(&mut st, targets, *op, values);
            }

            Stmt::Return { values, span } => ck.process_return(&mut st, values, *span),

            // O4.0 check-site: every task argument (HYB-E3001) + the
            // spawn-callee proof: the callee must resolve to a declared
            // top-level fn (a closure callee shares its captures with the
            // new task; an unknown callee is fail-closed, O4.9).
            Stmt::Task { callee, args, span } => {
                ck.mark_move_in_value(
                    &mut st,
                    callee,
                    format!("the spawn callee `{}`", callee.render()),
                );
                let desc = format!("the spawn `task {}(...)`", callee.render());
                let vals: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
                let targets = ck.spawn_param_shapes(&st, callee, args.len());
                ck.check_site(
                    &mut st,
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

            // O4.0 check-site: the sent value (HYB-E3002). The parser flags
            // `ch <- move x` as `is_move: true` with a plain Ident value —
            // `implicit_move` routes it to the class-3 move handling.
            Stmt::Send {
                chan,
                value,
                is_move,
                span,
            } => {
                ck.mark_move_in_value(
                    &mut st,
                    chan,
                    format!("the send target `{}`", chan.render()),
                );
                // Target classification (#77) runs FIRST: a non-channel (or
                // unresolvable) send target is ill-typed — no value cascade.
                if ck.check_send_target(&st, chan, *span) {
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
                    let targets = ck.chan_elem_shape(&st, chan);
                    ck.check_site(
                        &mut st,
                        *span,
                        "HYB-E3002",
                        &vals,
                        managed,
                        &desc,
                        *is_move,
                        None,
                        targets.as_deref(),
                    );
                    ck.check_chan_send_const_overflow(&st, chan, value);
                }
            }

            // Loop variables: fresh bindings per iteration shape.
            Stmt::For { kind, .. } => match kind {
                ForKind::Iterate { vars, expr } => {
                    ck.mark_move_in_value(
                        &mut st,
                        expr,
                        format!("the loop header `{}`", expr.render()),
                    );
                    let is_chan = matches!(ck.shape_of_expr(&st, expr), TypeShape::Chan(_));
                    let shapes = ck.iterate_var_shapes(&st, expr, vars.len());
                    for (name, shape) in vars.iter().zip(shapes) {
                        let is_ref = shape.is_reference_alias();
                        let may_share = ck.shape_may_share(&shape);
                        ck.define_binding(&mut st, name, shape, None, false, managed);
                        if !is_chan {
                            if may_share {
                                ck.record_may_share(&mut st, name, expr);
                            }
                            if is_ref {
                                if let Some(root) = expr.root_ident() {
                                    if ck.lookup_binding(&st, root).is_some() {
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
                ForKind::Range { var, lo, hi, .. } => {
                    ck.mark_move_in_value(
                        &mut st,
                        lo,
                        format!("the range bound `{}`", lo.render()),
                    );
                    ck.mark_move_in_value(
                        &mut st,
                        hi,
                        format!("the range bound `{}`", hi.render()),
                    );
                    ck.define_binding(&mut st, var, TypeShape::Primitive, None, false, managed);
                }
                ForKind::While { cond } => {
                    ck.mark_move_in_value(
                        &mut st,
                        cond,
                        format!("the loop condition `{}`", cond.render()),
                    );
                }
                ForKind::Infinite => {}
            },

            // select arms: `ch <- v` is a send check-site; `v, ok := <-ch`
            // defines its names inside that arm's own scope via
            // `ScopeEvent::SelectRecv` (see `walk.rs`).
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                            ..
                        } => {
                            ck.mark_move_in_value(
                                &mut st,
                                chan,
                                format!("the select-send target `{}`", chan.render()),
                            );
                            let span = chan.span().to(value.span());
                            if ck.check_send_target(&st, chan, span) {
                                // C5: a select send clause is a sendability
                                // check-site "exactly like statement sends", so
                                // the class-3 owned-move route (`ch <- move v`)
                                // must be available here too. The is_move marker
                                // the send-statement parser produced used to be
                                // dropped when the statement became a select
                                // arm: the move was invisible to the checker, so
                                // a legal owned move-send rejected as A1c and the
                                // sender's later use was never frozen (O5.2).
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
                                let vals: Vec<&Expr> = vec![value];
                                let targets = ck.chan_elem_shape(&st, chan);
                                ck.check_site(
                                    &mut st,
                                    span,
                                    "HYB-E3002",
                                    &vals,
                                    managed,
                                    &desc,
                                    *is_move,
                                    None,
                                    targets.as_deref(),
                                );
                                ck.check_chan_send_const_overflow(&st, chan, value);
                            }
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            ck.mark_move_in_value(
                                &mut st,
                                chan,
                                format!("the select-recv `{}`", chan.render()),
                            );
                        }
                        SelectArmKind::Default => {}
                    }
                }
            }

            // T13/T14/T15 (`HYB-E5001`): match exhaustiveness and `_` arm placement.
            Stmt::Match {
                subject,
                arms,
                span,
            } => {
                ck.mark_move_in_value(
                    &mut st,
                    subject,
                    format!("the match subject `{}`", subject.render()),
                );
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        ck.mark_move_in_value(
                            &mut st,
                            g,
                            format!("the match guard `{}`", g.render()),
                        );
                    }
                }
                ck.check_match(&st, subject, arms, *span);
            }

            Stmt::If { cond, .. } => {
                ck.mark_move_in_value(
                    &mut st,
                    cond,
                    format!("the condition `{}`", cond.render()),
                );
            }

            Stmt::Expr { expr, .. } => {
                ck.mark_move_in_value(
                    &mut st,
                    expr,
                    format!("the expression `{}`", expr.render()),
                );
            }

            Stmt::Defer { call, .. } => {
                ck.mark_move_in_value(
                    &mut st,
                    call,
                    format!("the deferred call `defer {}`", call.render()),
                );
            }

            // If-init Locals and blocks: already covered by the flat walk
            // (init is its own statement entry) and the uses/calls collected above.
            _ => {}
        }

        while let Some((_, loop_end, _, _, _)) = active_loops.last() {
            if *loop_end != idx {
                break;
            }
            let (for_idx, loop_end, outer_depth, lbody, pre_moved) = active_loops.pop().unwrap();
            ck.check_loop_backedge(
                &st,
                outer_depth,
                &flat.stmts[for_idx..=loop_end],
                &flat.events,
                lbody,
                &pre_moved,
                &mut reported_backedge,
            );
        }
    }

    // All remaining block scopes die at the end of the function (issue #14):
    // the closure pass's capture view is the COMPLETED base env (A5) — a
    // capture of a block-local now resolves to nothing and fails closed
    // (HYB-E3005) at the boundary, exactly like any unresolvable name.
    while !st.scopes.is_empty() {
        st.scope_pop();
    }

    // #123: closure bodies sit outside Pass B (A5), so process_assign and
    // process_call never see their writes — scan every FnLit for assign
    // targets / `delete()` calls through a `const` root with the completed
    // env (file consts + fn-local consts).
    ck.check_closure_const_writes(&st, body);

    // Closure-boundary pass (closures.rs): task/send/select-send STATEMENTS
    // inside closure bodies are concurrency boundaries in the emitted Go —
    // checked here, fail-closed (O4.9), with the completed function env as
    // the capture view. Captures/loans/freeze stay outside (A5, documented).
    closures::check_closure_boundaries(ck, &st, body);
}

impl<'a> Checker<'a> {
    /// Row 14 (discussion #4): audit a `managed` block's reason/category/tickets
    /// and record it in `managed_audit` (shared between Pass B and closures.rs).
    pub(crate) fn audit_managed_block(
        &mut self,
        reason: &str,
        category: &str,
        tickets: &[String],
        span: hy_diag::Span,
    ) {
        self.managed_blocks += 1; // O6.3: one per `managed` statement
        if reason.trim().is_empty() {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E0003",
                    self.path.clone(),
                    span,
                    "managed block has no reason — the escape hatch must say what it bypasses and why (HYB-E0003)",
                )
                .with_notes(vec![
                    "a bare `managed { ... }` is a reject, not a lint — the hatch must be review-visible (discussion #4)"
                        .to_string(),
                ]),
            );
        }
        if category.is_empty() {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E0003",
                    self.path.clone(),
                    span,
                    "managed block has no category — name the invariant being bypassed (HYB-E0003)",
                )
                .with_notes(vec![
                    "the category tells reviewers WHICH invariant the block bypasses and feeds `--report-managed`; without it the hatch cannot be audited".to_string(),
                    format!(
                        "fix: pass a category as the second argument, e.g. `managed(\"why this is safe\", \"mutex\") {{ ... }}`; valid categories: {}",
                        MANAGED_CATEGORIES.join(", ")
                    ),
                ]),
            );
        } else if !MANAGED_CATEGORIES.contains(&category) {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E0004",
                    self.path.clone(),
                    span,
                    format!(
                        "unknown managed category `\"{}\"` — use one of the listed categories, or `\"other\"` with the specifics in the reason (HYB-E0004)",
                        category
                    ),
                )
                .with_notes(vec![
                    format!(
                        "valid categories: {} — the category tells reviewers WHICH invariant the block bypasses; `other` requires the reason to carry the specifics",
                        MANAGED_CATEGORIES.join(", ")
                    ),
                ]),
            );
        }
        self.managed_audit.push(ManagedBlockAudit {
            reason: reason.to_string(),
            category: category.to_string(),
            tickets: tickets.to_vec(),
        });
    }
}

// ---------------------------------------------------------------------------
// Tests — parsed-snippet checks per rule family (verdict + first-error rule)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hy_syntax::parse_file;

    fn check(src: &str) -> CheckResult {
        let ast = parse_file("t.hy", src).expect("snippet parses");
        check_file(&ast)
    }

    fn first_error_rule(r: &CheckResult) -> Option<String> {
        r.diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .and_then(|d| d.tier0_rule.clone())
    }

    #[test]
    fn clean_program_accepts() {
        let r = check("fn main() { x := 1 \n fmt.println(x) }");
        assert_eq!(r.verdict, Verdict::Accept);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }


    #[test]
    fn shadow_scope_does_not_wipe_outer_alias_e3003() {
        // An inner block shadowing `m2 := 1` must not wipe the outer alias
        // `m2 -> m`: moving `m` after the block (or inside the block) must
        // still freeze outer `m2` (HYB-E3003).
        let r_after = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               m2 := m\n\
               if true {\n\
                 m2 := 1\n\
                 fmt.println(m2)\n\
               }\n\
               done := make(chan bool, 1)\n\
               task worker(move m, done)\n\
               m2[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_after.verdict,
            Verdict::Reject,
            "{:?}",
            r_after.diagnostics
        );
        assert_eq!(first_error_rule(&r_after).as_deref(), Some("A1b"));
        assert!(r_after.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r_inside = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               m2 := m\n\
               done := make(chan bool, 1)\n\
               if true {\n\
                 m2 := 1\n\
                 fmt.println(m2)\n\
                 task worker(move m, done)\n\
               }\n\
               m2[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_inside.verdict,
            Verdict::Reject,
            "{:?}",
            r_inside.diagnostics
        );
        assert_eq!(first_error_rule(&r_inside).as_deref(), Some("A1b"));
        assert!(r_inside.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn mutual_alias_rebind_cycle_still_freezes_on_move_e3003() {
        // `m2 := m; m = m2` used to create a 2-cycle `m <-> m2` where 16
        // hops of `resolve_alias` resolved `"m"` to `"m"` and `"m2"` to `"m2"`,
        // leaving `m2` unfrozen after `move m`.
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               m2 := m\n\
               m = m2\n\
               done := make(chan bool, 1)\n\
               task worker(move m, done)\n\
               m2[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn same_statement_double_move_and_move_plus_read_reject_e3003() {
        // Moving the same binding (or its alias) twice in one statement, or
        // moving and reading it in the same multi-define, must reject with HYB-E3003.
        let r_spawn = check(
            "fn both(a map[string]int, b map[string]int) { a[\"k\"] = 1\n b[\"k\"] = 2 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               task both(move m, move m)\n\
             }",
        );
        assert_eq!(
            r_spawn.verdict,
            Verdict::Reject,
            "{:?}",
            r_spawn.diagnostics
        );
        assert!(r_spawn.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r_multi = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               a, b := move m, move m\n\
               done := make(chan bool, 1)\n\
               task worker(move a, done)\n\
               b[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_multi.verdict,
            Verdict::Reject,
            "{:?}",
            r_multi.diagnostics
        );
        assert!(r_multi.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r_move_and_read = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               a, b := move m, m\n\
               done := make(chan bool, 1)\n\
               task worker(move a, done)\n\
               b[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_move_and_read.verdict,
            Verdict::Reject,
            "{:?}",
            r_move_and_read.diagnostics
        );
        assert!(r_move_and_read
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // Shadowing the outer root `m` (`m := 1`) inside a block before moving
        // an alias `m2` of the outer `m` must still freeze the outer `m`.
        let r_shadow_root = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               if true {\n\
                 m2 := m\n\
                 m := 1\n\
                 task worker(move m2, done)\n\
                 _ = m\n\
               }\n\
               m[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_shadow_root.verdict,
            Verdict::Reject,
            "{:?}",
            r_shadow_root.diagnostics
        );
        assert_eq!(first_error_rule(&r_shadow_root).as_deref(), Some("A1b"));
    }

    #[test]
    fn o52_struct_and_array_carrying_reference_fields_freeze_aliases_on_move() {
        // Shallow copy of a struct carrying a map shares the underlying map header;
        // moving h1 must freeze h2 (issue #27).
        let r_struct = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder) { h.m[\"k\"] = 1 }\n\
             fn main() {\n\
               h1 := holder{m: map[string]int{\"k\": 0}}\n\
               h2 := h1\n\
               task worker(move h1)\n\
               h2.m[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_struct.verdict,
            Verdict::Reject,
            "{:?}",
            r_struct.diagnostics
        );
        // the alias-freeze E3003 carries no tier0_rule on the current seam
        // (only consumed moves stamp A7) — the code is the stable contract
        assert!(r_struct.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // Positive control: a pure value struct (no reference fields) is deep-copied
        // by value in Go, so copying before `move` stays legal.
        let r_value_struct = check(
            "struct point { x int\n y int }\n\
             fn worker(p point) { fmt.println(p.x) }\n\
             fn main() {\n\
               p1 := point{x: 1, y: 2}\n\
               p2 := p1\n\
               task worker(move p1)\n\
               fmt.println(p2.y)\n\
             }",
        );
        assert_eq!(
            r_value_struct.verdict,
            Verdict::Accept,
            "{:?}",
            r_value_struct.diagnostics
        );
    }

    #[test]
    fn o52_paren_slice_projection_and_composite_literal_aliases_freeze_on_move() {
        // 1. Parenthesized alias: `m2 := (agg)`
        let r_paren = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               agg := map[string]int{\"k\": 0}\n\
               m2 := (agg)\n\
               task worker(move agg)\n\
               m2[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_paren.verdict,
            Verdict::Reject,
            "{:?}",
            r_paren.diagnostics
        );
        assert!(r_paren.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 2. Re-sliced alias: `s2 := s1[:]`
        let r_slice = check(
            "fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s1 := []int{0, 1}\n\
               s2 := s1[:]\n\
               task worker(move s1)\n\
               s2[0] = 2\n\
             }",
        );
        assert_eq!(
            r_slice.verdict,
            Verdict::Reject,
            "{:?}",
            r_slice.diagnostics
        );
        assert!(r_slice.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 3. Field projection of reference type: `m2 := h1.m`
        let r_proj = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder) { h.m[\"k\"] = 1 }\n\
             fn main() {\n\
               h1 := holder{m: map[string]int{\"k\": 0}}\n\
               m2 := h1.m\n\
               task worker(move h1)\n\
               m2[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r_proj.verdict, Verdict::Reject, "{:?}", r_proj.diagnostics);
        assert!(r_proj.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 4. Composite literal capturing a live mutable binding without `move`: `h := holder{m: agg}`
        let r_comp = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder) { h.m[\"k\"] = 1 }\n\
             fn main() {\n\
               agg := map[string]int{\"k\": 0}\n\
               h := holder{m: agg}\n\
               task worker(move h)\n\
               agg[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r_comp.verdict, Verdict::Reject, "{:?}", r_comp.diagnostics);
        assert!(r_comp.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 5. Composite literal capturing a field projection (`holder{m: h1.m}`)
        let r_comp_proj = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder) { h.m[\"k\"] = 1 }\n\
             fn main() {\n\
               h1 := holder{m: map[string]int{\"k\": 0}}\n\
               h2 := holder{m: h1.m}\n\
               task worker(move h2)\n\
               h1.m[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_comp_proj.verdict,
            Verdict::Reject,
            "{:?}",
            r_comp_proj.diagnostics
        );
        assert!(r_comp_proj
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn composite_literal_nil_fields_reject_e4001() {
        // PLAN follow-up (the audit's composite-nil row): classify_composite
        // has no per-field type context, so `job{m: nil}` passed with 0
        // diagnostics. The literal carries its own type context — the nil
        // rejects as a TYPE error wherever the slot resolves.
        // S1: the audit repro — spawn arg literal.
        let r_spawn = check(
            "struct job { m map[string]int }\n\
             fn worker(j job) { j.m[\"k\"] = 1 }\n\
             fn main() {\n\
               j := job{m: nil}\n\
               task worker(j)\n\
             }",
        );
        assert_eq!(r_spawn.verdict, Verdict::Reject, "{:?}", r_spawn.diagnostics);
        assert!(r_spawn.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        // S2: a plain local define — no boundary involved.
        let r_local = check(
            "struct job { m map[string]int }\n\
             fn main() {\n\
               j := job{m: nil}\n\
               fmt.println(j)\n\
             }",
        );
        assert_eq!(r_local.verdict, Verdict::Reject, "{:?}", r_local.diagnostics);
        assert!(r_local.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        // S3: map literal value slot + slice element slot.
        let r_map = check("fn main() {\n  mm := map[string][]int{\"k\": nil}\n  fmt.println(mm)\n}");
        assert_eq!(r_map.verdict, Verdict::Reject, "{:?}", r_map.diagnostics);
        assert!(r_map.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        let r_slice = check("fn main() {\n  xs := []map[string]int{nil}\n  fmt.println(xs)\n}");
        assert_eq!(r_slice.verdict, Verdict::Reject, "{:?}", r_slice.diagnostics);
        assert!(r_slice.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        // S4: the managed hatch does NOT suppress a type contract.
        let r_managed = check(
            "struct job { m map[string]int }\n\
             fn worker(j job) { j.m[\"k\"] = 1 }\n\
             fn main() {\n\
               managed(\"the job is rebuilt inside the worker\", \"actor\") {\n\
                 task worker(job{m: nil})\n\
               }\n\
             }",
        );
        assert_eq!(
            r_managed.verdict,
            Verdict::Reject,
            "{:?}",
            r_managed.diagnostics
        );
        assert!(r_managed
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E4001"));

        // Nested literals reject too.
        let r_nested = check(
            "struct job { m map[string]int }\n\
             struct wrap { j job }\n\
             fn main() {\n\
               w := wrap{j: job{m: nil}}\n\
               fmt.println(w)\n\
             }",
        );
        assert_eq!(r_nested.verdict, Verdict::Reject, "{:?}", r_nested.diagnostics);
        assert!(r_nested.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        // Positive control: a real map value stays legal; error-typed nil
        // stays legal (N2).
        let r_ok = check(
            "struct job { m map[string]int }\n\
             fn main() {\n\
               j := job{m: map[string]int{}}\n\
               fmt.println(j)\n\
             }",
        );
        assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
    }

    #[test]
    fn closure_boundary_rejects_outer_or_closure_moved_binding_e3003() {
        // Issue #29: outer binding `m` is moved in `main`, and closure `f`
        // spawns a task with `m` — must not silently pass via `b.moved.is_some() => Ok`.
        let r_outer = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 2)\n\
               f := fn() {\n\
                 task worker(m, done)\n\
               }\n\
               task worker(move m, done)\n\
               f()\n\
             }",
        );
        assert_eq!(
            r_outer.verdict,
            Verdict::Reject,
            "{:?}",
            r_outer.diagnostics
        );
        assert_eq!(first_error_rule(&r_outer).as_deref(), Some("A1b"));
        assert!(r_outer.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // Closure-local binding moved once then shared/moved again at a second
        // boundary inside the same closure must also reject with HYB-E3003.
        let r_inner = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               done := make(chan bool, 2)\n\
               f := fn() {\n\
                 m := map[string]int{\"k\": 0}\n\
                 task worker(move m, done)\n\
                 task worker(m, done)\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(
            r_inner.verdict,
            Verdict::Reject,
            "{:?}",
            r_inner.diagnostics
        );
        assert_eq!(first_error_rule(&r_inner).as_deref(), Some("A1b"));
        assert!(r_inner.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // Moving a captured outer binding inside a closure (`task worker(move m)`)
        // must also reject with HYB-E3003 / HYB-E3004 (A1b) because closures can
        // run multiple times or concurrently with the enclosing scope.
        let r_capture_move = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 2)\n\
               f := fn() {\n\
                 task worker(move m, done)\n\
               }\n\
               f()\n\
               m[\"k\"] = 2\n\
             }",
        );
        assert_eq!(
            r_capture_move.verdict,
            Verdict::Reject,
            "{:?}",
            r_capture_move.diagnostics
        );
        assert_eq!(first_error_rule(&r_capture_move).as_deref(), Some("A1b"));
        assert!(r_capture_move
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003" || d.code == "HYB-E3004"));

        // Using/mutating a closure-local binding after moving it inside the
        // closure must reject with HYB-E3003 even when the use is a non-boundary
        // statement (`m["k"] = 2`).
        let r_inner_assign = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 m := map[string]int{\"k\": 0}\n\
                 task worker(move m, done)\n\
                 m[\"k\"] = 2\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(
            r_inner_assign.verdict,
            Verdict::Reject,
            "{:?}",
            r_inner_assign.diagnostics
        );
        assert_eq!(first_error_rule(&r_inner_assign).as_deref(), Some("A1b"));

        // A closure defined inside a `select` arm body must still be collected
        // and checked by the closure boundary pass.
        let r_select_closure = check(
            "fn worker(m map[string]int, done chan<- bool) { m[\"k\"] = 1\n done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               select {\n\
               v, ok := <-done: {\n\
                   f := fn() {\n\
                     task worker(m, done)\n\
                   }\n\
                   f()\n\
                 }\n\
               }\n\
             }",
        );
        assert_eq!(
            r_select_closure.verdict,
            Verdict::Reject,
            "{:?}",
            r_select_closure.diagnostics
        );
        assert_eq!(first_error_rule(&r_select_closure).as_deref(), Some("A1c"));
    }

    #[test]
    fn move_cannot_launder_closure_borrow_handle_or_unknown_at_boundary() {
        let r_closure = check(
            "fn run(f fn()) { f() }\n\
             fn main() {\n\
               m := map[string]int{}\n\
               f := fn() { m[\"k\"] = 1 }\n\
               task run(move f)\n\
               m[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r_closure.verdict, Verdict::Reject);
        // AMENDMENT (pr-110 seam): the closure-capture share edge makes the
        // freeze fire at the MOVE (A1b/E3003) BEFORE the spawn-boundary
        // A1c/E3001 — both rules must be present, A1b now first. The old
        // assertion pinned A1c-first because the capture edge did not exist.
        assert_eq!(first_error_rule(&r_closure).as_deref(), Some("A1b"));
        assert!(r_closure
            .diagnostics
            .iter()
            .any(|d| d.tier0_rule.as_deref() == Some("A1c")),
            "the spawn-boundary A1c diagnostic must still fire: {:?}",
            r_closure.diagnostics);

        let r_ptr = check(
            "struct st { n int }\n\
             fn take(r *st) {}\n\
             fn main() {\n\
               x := st{}\n\
               r := &x\n\
               task take(move r)\n\
               x.n = 2\n\
             }",
        );
        assert_eq!(r_ptr.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r_ptr).as_deref(), Some("A1b"));

        let r_borrow = check(
            "import \"rust/serde_json\"\n\
             fn send_val(v any) {}\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               v := doc.get(\"k\")?\n\
               task send_val(move v)\n\
               doc.set(\"k\", 1)?\n\
             }",
        );
        assert_eq!(r_borrow.verdict, Verdict::Reject);

        // Fixture note (issue #228 family A): the unproven value is a
        // DEFINED binding of Unknown shape (recv from an unresolvable-elem
        // channel — the same idiom as fc_select_recv_unknown_element above),
        // not an undefined factory call: HYB-E1001 owns names nothing
        // declares, so it would (correctly) become the first error here.
        let r_unknown = check(
            "fn send_val(v any) {}\n\
             fn main() {\n\
               ch := make(chan mystery, 1)\n\
               u := <-ch\n\
               task send_val(move u)\n\
             }",
        );
        assert_eq!(r_unknown.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r_unknown).as_deref(), Some("FC"));
    }

    #[test]
    fn a1c_spawn_shared_map_rejects() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task tally(counts)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3001");
        assert!(d.message.contains("counts"));
        assert!(d.message.contains("map[string]int"));
        assert!(!d.fixits.is_empty(), "O17: rejection must teach");
    }

    #[test]
    fn a1c_send_shared_struct_rejects() {
        let r = check(
            "struct s { n int }\n\
             fn main() {\n\
               st := s{}\n\
               ch := make(chan s, 1)\n\
               ch <- st\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3002");
    }

    #[test]
    fn a1c_addr_of_struct_rejects() {
        let r = check(
            "struct st { n int }\n\
             fn w(s *st) { s.n++ }\n\
             fn main() {\n\
               stats := st{}\n\
               task w(&stats)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert!(r.diagnostics[0].message.contains("&stats"));
        assert!(r.diagnostics[0].message.contains("*st"));
    }

    #[test]
    fn a1c_fresh_literal_and_move_and_chan_ok() {
        let r = check(
            "struct s { a int }\n\
             fn w(x s, c chan<- s) { c <- s{a: 1} }\n\
             fn main() {\n\
               v := s{a: 2}\n\
               c := make(chan s, 1)\n\
               task w(move v, c)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn a1b_write_after_move_send() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               agg[\"w\"]++\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               agg[\"total\"] = len(agg)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3003");
        assert!(d.message.contains("agg"));
    }

    #[test]
    fn a1b_read_after_move_send() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               n := len(agg)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn a1b_alias_write_after_move() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               m2 := agg\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               m2[\"total\"] = len(m2)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert!(d.message.contains("alias"), "{}", d.message);
    }

    #[test]
    fn a1b_call_returned_alias_is_frozen_by_move() {
        // Issue #38: a call that returns a reference-typed value may hand
        // back storage owned by an argument (`same(m)`), so `move m` must
        // freeze the call result too — no interprocedural summary exists to
        // prove the return is fresh.
        let r = check(
            "fn same(m map[string]int) map[string]int {\n\
               return m\n\
             }\n\
             fn main() {\n\
               agg := map[string]int{}\n\
               results := make(chan map[string]int, 1)\n\
               alias := same(agg)\n\
               results <- move agg\n\
               alias[\"total\"] = len(alias)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert!(d.message.contains("alias"), "{}", d.message);
    }

    #[test]
    fn a1b_call_returned_fresh_map_not_frozen() {
        // Negative control: the result does not alias any reference-typed
        // argument, so moving a primitive argument does not freeze it.
        let r = check(
            "fn fresh(seed int) map[string]int {\n\
               return map[string]int{\"s\": seed}\n\
             }\n\
             fn main() {\n\
               n := 1\n\
               results := make(chan int, 1)\n\
               alias := fresh(n)\n\
               results <- move n\n\
               alias[\"x\"] = 1\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a2_mutate_during_loan_rejects() {
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               for u in doc.array(\"users\")? {\n\
                 if u.int(\"logins\")? == 0 {\n\
                   doc.set(\"pruned\", true)?\n\
                 }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A2"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3101");
        assert!(d.message.contains("doc"));
    }

    #[test]
    fn a2_mutate_after_loan_dies_accepts() {
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               for u in doc.array(\"users\")? {\n\
                 fmt.println(u.int(\"logins\")?)\n\
               }\n\
               doc.set(\"pruned\", true)?\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a3_insert_inside_window_rejects() {
        let r = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               s := &e.m[\"a\"]\n\
               e.m[\"b\"] = svc{n: 1}\n\
               s.n = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A3"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3102");
        assert!(d.message.contains("insert"));
    }

    #[test]
    fn a3_assign_rebind_loan_window_rejects() {
        // Issue #138: re-assignment `s = &e.m["a"]` opens the same loan window
        // as the define `s := &e.m["a"]`.
        let r = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               var s *svc\n\
               s = &e.m[\"a\"]\n\
               e.m[\"b\"] = svc{n: 1}\n\
               s.n = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A3"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3102");
        assert!(d.message.contains("insert"));
    }

    #[test]
    fn a3_insert_after_last_use_accepts() {
        let r = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               s := &e.m[\"a\"]\n\
               s.n = 2\n\
               e.m[\"b\"] = svc{n: 1}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a6_borrow_handle_field_rejects() {
        let r = check(
            "import \"rust/serde_json\"\n\
             struct auditor {\n\
               trail []string\n\
               cur   serde_json.view\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A6"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3103");
        assert!(d.message.contains("auditor.cur"));
        assert!(d.message.contains("serde_json.view"));
    }

    #[test]
    fn loan_escape_return_addr_of_rejects() {
        let r = check(
            "struct c { hits int }\n\
             struct reg { cs []c }\n\
             fn get(r *reg, i int) *c {\n\
               return &r.cs[i]\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("LOAN-ESCAPE"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3104");
        assert!(d.message.contains("borrow"));
    }

    #[test]
    fn a7_use_after_consuming_rejects() {
        // Fixture seam-fix for #134: `out` must be read (`_ = out`), or the
        // snippet itself is a declared-and-not-used program and the HYB-E1002
        // buildability reject lands first. The A7 consume rule is unchanged.
        let r = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               out := enc.finish()?\n\
               _ = out\n\
               enc.write([]byte(\"trailer\"))?\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A7"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3003");
        assert!(d.message.contains("enc"));
    }

    #[test]
    fn a7_borrow_by_default_and_return_transfer_accept() {
        // 12-shape: two borrow calls then a mutate — windows non-lexical.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn count(doc serde_json.value) int {\n\
               n := 0\n\
               for u in doc.array(\"users\")? {\n\
                 n += u.int(\"logins\")?\n\
               }\n\
               return n\n\
             }\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               a := count(doc)?\n\
               b := count(doc)?\n\
               doc.set(\"audited\", true)?\n\
               fmt.println(a + b)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn managed_suppression_lints_and_accepts() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed(\"counters merged under the test's own mutex\", \"mutex\", \"PROJ-42\") {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
            "managed lint must fire: {:?}",
            r.diagnostics
        );
        assert!(r.diagnostics.iter().all(|d| d.severity != Severity::Error));
        assert_eq!(r.managed.managed_blocks, 1);
        assert!(r.managed.sites_via_managed >= 1);
        assert!(r.managed.total_sites >= 1);
        // row 14: the audit is a table, not a bare number
        assert_eq!(r.managed.categories.get("mutex"), Some(&1));
        assert_eq!(r.managed.blocks.len(), 1);
        assert_eq!(r.managed.blocks[0].tickets, vec!["PROJ-42".to_string()]);
        assert!(!r.managed.blocks[0].reason.is_empty());
    }

    #[test]
    fn managed_bare_block_rejects_e0003() {
        // row 14 fail-closed: no payload = coded reject
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let codes: Vec<&str> = r.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"HYB-E0003"), "{:?}", r.diagnostics);
        // BOTH missing parts are named (reason + category)
        assert_eq!(
            r.diagnostics
                .iter()
                .filter(|d| d.code == "HYB-E0003")
                .count(),
            2
        );
    }

    #[test]
    fn managed_unknown_category_rejects_e0004() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed(\"r\", \"misc\") {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E0004"),
            "{:?}",
            r.diagnostics
        );
        let e4 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E0004")
            .unwrap();
        assert!(e4.notes[0].contains("mutex"), "fix-it lists the set");
    }

    #[test]
    fn managed_e0003_e0004_wording_teaches_the_fix() {
        // Regression pin: the no-category E0003 note and the E0004 headline
        // must tell the user HOW to fix it, not just what is wrong.
        let body = "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
                    fn main() {\n\
                      managed {\n\
                        counts := map[string]int{}\n\
                        task tally(counts)\n\
                      }\n\
                    }";
        let r = check(body);
        let e3 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E0003" && d.message.contains("no category"))
            .expect("no-category E0003");
        assert!(
            e3.notes.iter().any(|n| n.contains("--report-managed")),
            "{:?}",
            e3.notes
        );
        assert!(
            e3.notes.iter().any(|n| n.contains("fix: pass a category")
                && n.contains("managed(\"why this is safe\", \"mutex\")")
                && n.contains("valid categories:")),
            "{:?}",
            e3.notes
        );

        let r = check(&body.replace("managed {", "managed(\"r\", \"misc\") {"));
        let e4 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E0004")
            .expect("E0004");
        assert_eq!(
            e4.message,
            "unknown managed category `\"misc\"` \u{2014} use one of the listed categories, or `\"other\"` with the specifics in the reason (HYB-E0004)"
        );
    }

    #[test]
    fn atomic_and_const_immutable_spawn_args_accept() {
        // 17-shape: `&hits` where hits is atomic-typed; const table read.
        let r = check(
            "import \"sync/atomic\"\n\
             const th = [10, 50, 250, 1000]\n\
             struct sample { probe int }\n\
             fn probe(id int, hits *atomic.int64, s chan<- sample) {\n\
               if th[3] > 100 {\n\
                 hits.add(1)\n\
               }\n\
               s <- sample{probe: id}\n\
             }\n\
             fn main() {\n\
               var hits atomic.int64\n\
               samples := make(chan sample, 4)\n\
               task probe(0, &hits, samples)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a2_define_stored_view_extends_loan() {
        // `u := doc.array(...)` stores an element view: the borrow loan on
        // the handle lives until the view's last use (O7 non-lexical) — a
        // mutating call inside that window conflicts.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               u := doc.array(\"users\")?\n\
               doc.set(\"x\", true)?\n\
               fmt.println(u.int(\"n\")?)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A2"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3101");
    }

    #[test]
    fn a2_primitive_boundary_read_is_a_copy() {
        // O9: a boundary read of a primitive is a COPY — it must NOT keep
        // the handle borrowed; the later mutating call is legal.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               n := doc.int(\"n\")?\n\
               doc.set(\"x\", true)?\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a7_own_transfer_at_return_accepts() {
        // 13-shape: `return v` transfers the own handle OUT; the caller
        // mutates the returned handle — the transfer made it the caller's.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn summary(doc serde_json.value) serde_json.value {\n\
               v := serde_json.new_object()\n\
               for u in doc.array(\"users\")? {\n\
                 if u.int(\"logins\")? > 0 {\n\
                   v.set(\"active\", 1)\n\
                 }\n\
               }\n\
               return v\n\
             }\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               s := summary(doc)?\n\
               s.set(\"cached_at\", \"2026-10-12\")?\n\
               fmt.println(serde_json.to_string_pretty(s)?)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn o46_deep_sendability_of_composites() {
        // A fresh literal is NOT automatically safe: embedding a live
        // mutable map by name is rejected, naming that part (O4.6).
        let r = check(
            "struct job { data map[string]int }\n\
             fn w(j job) { }\n\
             fn main() {\n\
               shared_map := map[string]int{}\n\
               ch := make(chan job, 1)\n\
               ch <- job{data: shared_map}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3002");
        assert!(d.message.contains("shared_map"), "{}", d.message);
    }

    #[test]
    fn fresh_literal_with_sendable_parts_accepts() {
        // The clean twin: every reference-typed part sendable.
        let r = check(
            "struct probe { id int }\n\
             struct sample { p probe }\n\
             fn w(s chan<- sample) { }\n\
             fn main() {\n\
               id := 3\n\
               c := make(chan sample, 1)\n\
               task w(c)\n\
               c <- sample{p: probe{id: id}}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    // -------------------------------------------------------------------
    // O4.9 fail-closed boundary (rule FC, HYB-E3005) — human directive
    // 2026-10-13: what the checker cannot prove, it does not call safe.
    // The escape is the explicit, auditable `managed` hatch (HYB-E8001).
    // -------------------------------------------------------------------

    #[test]
    fn fc_unknown_initializer_at_boundary_rejects() {
        // the initializer's type does not resolve (unstubbed foreign call) →
        // the binding is Unknown → crossing is E3005, not a silent pass
        let r = check(
            "fn handle(x string) { fmt.println(x) }\n\
             fn main() {\n\
               blob := strings.repeat(\"a\", 3)\n\
               task handle(blob)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("E3005 must fire for the unproven value");
        assert_eq!(d.tier0_rule.as_deref(), Some("FC"));
        assert!(d.message.contains("blob"), "{}", d.message);
        assert!(
            !d.fixits.is_empty(),
            "O17: the diagnostic must teach the fix"
        );
    }

    #[test]
    fn fc_unresolved_name_at_boundary_rejects() {
        // a name that does not resolve cannot be proven safe — it used to
        // pass silently (Finding 1, sendability.rs)
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               task handle(nosuch)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_e3005_fixit_suggests_current_managed_syntax() {
        // Issue #7 (bounty wording sweep): the HYB-E3005 fix-it used to tell
        // readers to write a bare `managed { ... }` — syntax the checker now
        // rejects outright (HYB-E0003: reason + category are required, see
        // fc_unresolved_name_at_boundary_rejects' sibling test below). The
        // fix-it must name the hatch that actually compiles.
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               task handle(nosuch)\n\
             }",
        );
        let e3005 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("unresolved name at a spawn boundary is HYB-E3005");
        assert!(
            e3005
                .notes
                .iter()
                .any(|n| n.contains("managed(\"why this bypass is safe\", \"category\")")),
            "fix-it must suggest the real managed(reason, category) syntax: {:?}",
            e3005.notes
        );
        assert!(
            !e3005.notes.iter().any(|n| n.contains("managed { ")),
            "fix-it must not suggest the bare `managed {{ ... }}` form, which HYB-E0003 rejects: {:?}",
            e3005.notes
        );
    }

    #[test]
    fn fc_unproven_name_via_managed_escapes() {
        // the SAME unproven value inside a managed block escapes through the
        // explicit hatch: warning only, counted — never a silent pass.
        // Fixture note (issue #228 family A): the unproven value is now a
        // DEFINED binding of Unknown shape (`v := <-ch` with an unresolvable
        // element type) instead of the old undefined-name placeholder —
        // HYB-E1001 rejects a name nothing declares, which is a different
        // (and also correct) claim than the managed-hatch claim under test.
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               managed(\"test-only handler; name shape pinned by the fixture\", \"other\") {\n\
                 ch := make(chan mystery, 1)\n\
                 v := <-ch\n\
                 task handle(v)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E8001"));
        assert!(r.diagnostics.iter().all(|d| d.severity != Severity::Error));
        assert_eq!(r.managed.managed_blocks, 1);
        assert_eq!(r.managed.sites_via_managed, 1);
    }

    #[test]
    fn fc_ascription_makes_the_share_provable() {
        // the fix-it from the E3005 diagnostic: ascribe the type at the
        // declaration — the checker can now classify (string = class 4)
        let r = check(
            "fn handle(x string) { fmt.println(x) }\n\
             fn main() {\n\
               var blob string = strings.repeat(\"a\", 3)\n\
               task handle(blob)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn fc_select_recv_known_element_classifies() {
        // `v, ok := <-ch` — the value's shape is the channel's ELEMENT when
        // the channel resolves (no manufactured unknown)
        let r = check(
            "fn take(x int) { fmt.println(x) }\n\
             fn main() {\n\
               ch := make(chan int, 1)\n\
               select {\n\
               v, ok := <-ch: { task take(v) }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn fc_select_recv_unknown_element_rejects_at_boundary() {
        // the channel's element did not resolve → v is Unknown → v crossing
        // is E3005 (this surface used to define a permissive Unknown)
        let r = check(
            "fn take(x int) { fmt.println(x) }\n\
             fn main() {\n\
               ch := make(chan mystery, 1)\n\
               select {\n\
               v, ok := <-ch: { task take(v) }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_closure_boundary_send_of_shared_map_rejects() {
        // Finding 2: a send INSIDE a closure body is a real boundary in the
        // emitted Go — the closures.rs pass owns it now
        let r = check(
            "fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 ch <- counts\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3002")
            .expect("the closure-body send must be checked");
        assert!(d.message.contains("counts"), "{}", d.message);
    }

    #[test]
    fn fc_closure_boundary_unproven_capture_rejects() {
        // fail-closed inside closures too: an unresolved capture at a
        // closure-body boundary is E3005, not a silent pass
        let r = check(
            "fn main() {\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 ch <- ghost\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_closure_boundary_send_inside_managed_escapes() {
        // the hatch works inside closures (managed_blocks parity: the
        // closure pass counts and audits its managed blocks for --report-managed)
        let r = check(
            "fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 managed(\"handler serializes access\", \"mutex\") { ch <- counts }\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E8001"));
        assert_eq!(r.managed.managed_blocks, 1);
        assert_eq!(r.managed.sites_via_managed, 1);
        assert_eq!(r.managed.categories.get("mutex"), Some(&1));
        assert_eq!(r.managed.blocks.len(), 1);
    }

    #[test]
    fn fc_closure_without_boundary_stays_outside_perimeter() {
        // corpus 10 shape unchanged: a capture used INSIDE a closure without
        // any task/send statement is still the A5 perimeter (Q1, OQ1) — the
        // closure pass owns boundary statements, nothing more
        let r = check(
            "fn main() {\n\
               hits := 0\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/ping\", http.handler(fn(w http.response_writer) {\n\
                 hits++\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn closure_capture_is_outside_perimeter() {
        // 10-shape: FnLit bodies are never analyzed (A5 scoped claim).
        let r = check(
            "fn main() {\n\
               hits := 0\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/ping\", http.handler(fn(w http.response_writer) {\n\
                 hits++\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn diagnostics_are_sorted_in_source_order() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task tally(counts)\n\
               results := make(chan map[string]int, 1)\n\
               results <- move counts\n\
               n := len(counts)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        let spans: Vec<_> = r
            .diagnostics
            .iter()
            .map(|d| (d.span.start.line, d.span.start.col))
            .collect();
        let mut sorted = spans.clone();
        sorted.sort();
        assert_eq!(spans, sorted);
        // first error in source order is the spawn (A1c)
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn o54_move_on_gc_field_at_spawn_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               h := holder{data: map[string]int{\"k\": 0}}\n\
               task worker(move h.data)\n\
               h.data[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o54_move_on_gc_field_at_send_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn main() {\n\
               s := holder{data: map[string]int{}}\n\
               jobs := make(chan map[string]int, 1)\n\
               jobs <- move s.data\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o54_move_on_gc_field_at_local_define_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn main() {\n\
               s := holder{data: map[string]int{}}\n\
               m := move s.data\n\
               fmt.println(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o51_parenthesized_and_composite_nested_move_freezes_binding() {
        let r1 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task worker(move (counts))\n\
               counts[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r2 = check(
            "struct job { data map[string]int }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan job, 1)\n\
               ch <- job{data: move counts}\n\
               counts[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn o51_move_inside_loop_of_outer_binding_rejects_e3003() {
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               for i in 0..10 { m[\"k\"] = i }\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {\n\
                 task worker(move m, done)\n\
               }\n\
               <-done\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3003");
        assert!(d.message.contains("moved again"), "{}", d.message);
    }

    #[test]
    fn o52_use_before_move_inside_loop_rejects_on_backedge() {
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) { done <- true }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {\n\
                 m[\"k\"] = i\n\
                 task worker(move m, done)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    #[test]
    fn o51_fresh_binding_inside_loop_and_move_then_break_accept() {
        let r_fresh = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               for i in 0..2 {\n\
                 chunk := map[string]int{\"k\": i}\n\
                 task worker(move chunk)\n\
               }\n\
             }",
        );
        assert_eq!(r_fresh.verdict, Verdict::Accept, "{:?}", r_fresh.diagnostics);

        let r_break = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               for i in 0..2 {\n\
                 task worker(move m)\n\
                 break\n\
               }\n\
             }",
        );
        assert_eq!(r_break.verdict, Verdict::Accept, "{:?}", r_break.diagnostics);
    }

    // -----------------------------------------------------------------------
    // Scope-aware bindings (issue #14, LubuSeb's soundness report): an
    // inner-block define must shadow, never overwrite. The flat env let
    // `m := 1` inside an `if` rewrite the outer map's binding, and the
    // later `task worker(m)` classified against the INNER int and passed
    // with zero diagnostics while the emitted Go raced (go -race: two
    // write/write DATA RACEs).
    // -----------------------------------------------------------------------

    #[test]
    fn shadow_scope_does_not_launder_outer_map_e3001() {
        // The exact issue-#14 repro: the inner `m := 1` shadows the outer
        // map; the spawn AFTER the block must still see the MAP.
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               for i in 0..10000 { m[\"k\"] = i }\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 fmt.println(m)\n\
               }\n\
               done := make(chan bool, 1)\n\
               task worker(m, done)\n\
               for i in 0..10000 { m[\"k\"] = i }\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn shadow_inner_primitive_is_sendable_inside_block() {
        // The inner binding itself is a plain int: a boundary INSIDE the
        // block classifies against it and must stay accepted (no
        // over-rejection of legal shadowing).
        let r = check(
            "fn take(i int) { fmt.println(i) }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 task take(m)\n\
               }\n\
               fmt.println(len(m))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn shadow_does_not_leak_across_sibling_blocks() {
        // Two sibling ifs each define their own m; the second block must
        // not inherit the first's binding, and the outer map after both
        // must still reject at the spawn.
        let r = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn take(i int) { fmt.println(i) }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 task take(m)\n\
               }\n\
               if true {\n\
                 m := 2\n\
                 task take(m)\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn if_init_define_does_not_overwrite_outer_binding() {
        // An if-init define shares Go's if-scope; after the if, the outer
        // binding is intact and the spawn must classify against the MAP,
        // even when the if-init variable itself shadows `m` (both the
        // `then` body scope and the `if` scope pop at the same index).
        let r = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if m := 1; m > 0 {\n\
                 fmt.println(m)\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn for_var_and_nested_block_shadow_do_not_leak_outer_map() {
        // Both `for m in 0..1 { ... }` and `{ { m := 1; ... } }` close two
        // nested scopes at the same statement index; neither may leave the
        // inner `m: Primitive` scope on the stack after the construct ends.
        let r1 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               for m in 0..1 {\n\
                 fmt.println(m)\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert_eq!(first_error_rule(&r1).as_deref(), Some("A1c"));
        assert_eq!(r1.diagnostics[0].code, "HYB-E3001");

        let r2 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               { { m := 1\n fmt.println(m) } }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert_eq!(first_error_rule(&r2).as_deref(), Some("A1c"));
        assert_eq!(r2.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn select_recv_define_stays_scoped_to_its_arm() {
        // A `select` arm receive (`m := <-ch:`) is scoped to that arm's body:
        // it must not overwrite an outer `m` after the select or overwrite a
        // sibling arm's `m` before that arm's body is checked.
        let r1 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               ch := make(chan int, 1)\n\
               select {\n\
               m := <-ch: { fmt.println(m) }\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert_eq!(first_error_rule(&r1).as_deref(), Some("A1c"));
        assert_eq!(r1.diagnostics[0].code, "HYB-E3001");

        let r2 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               map_ch := make(chan map[string]int, 1)\n\
               int_ch := make(chan int, 1)\n\
               select {\n\
               m := <-map_ch: { task worker(m) }\n\
               m := <-int_ch: { fmt.println(m) }\n\
               }\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert_eq!(first_error_rule(&r2).as_deref(), Some("A1c"));
        assert_eq!(r2.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn closure_select_arm_body_boundaries_and_recv_scope_are_checked() {
        // Inside a closure body (closures.rs), `Stmt::Select` must both walk
        // each arm's body for boundary statements (`task` / `send`) and scope
        // `SelectArmKind::Recv` defines to their arm.
        let r1 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               ch := make(chan int, 1)\n\
               f := fn() {\n\
                 select {\n\
                 <-ch: { task worker(m) }\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert_eq!(first_error_rule(&r1).as_deref(), Some("A1c"));
        assert_eq!(r1.diagnostics[0].code, "HYB-E3001");

        let r2 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               ch := make(chan int, 1)\n\
               f := fn() {\n\
                 m := map[string]int{\"k\": 0}\n\
                 select {\n\
                 m := <-ch: { fmt.println(m) }\n\
                 }\n\
                 task worker(m)\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert_eq!(first_error_rule(&r2).as_deref(), Some("A1c"));
        assert_eq!(r2.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn let_in_closure_pass_is_shallow_too() {
        // The closure-boundary pass must agree with the flat pass on T22
        // (issue #12 parity): a let-bound map captured... here, a let-bound
        // map defined in the closure body itself is NOT class-4.
        let r = check(
            "fn main() {\n\
               fn() {\n\
                 let m = map[string]int{\"k\": 0}\n\
                 task bump(m)\n\
               }()\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    }

    // -----------------------------------------------------------------------
    // `new(T)` returns `*T`, not a primitive (O4 / O4.9). Regression for the
    // accepted-but-unsound shape `p := new(S); task w(p)`, which shared a
    // `*S` across tasks with zero diagnostics and races under `go -race`.
    // -----------------------------------------------------------------------

    #[test]
    fn new_builtin_spawn_pointer_rejects() {
        let r = check(
            "struct pool_stats { processed int }\n\
             fn bump(s *pool_stats) { s.processed = s.processed + 1 }\n\
             fn main() {\n\
               p := new(pool_stats)\n\
               task bump(p)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("*pool_stats"));
    }

    #[test]
    fn new_builtin_send_pointer_rejects() {
        let r = check(
            "fn main() {\n\
               jobs := make(chan int, 1)\n\
               p := new(int)\n\
               jobs <- p\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3002");
    }

    #[test]
    fn new_builtin_local_only_use_accepts() {
        // No boundary: a locally-confined `new` must not be rejected.
        let r = check(
            "struct pool_stats { processed int }\n\
             fn main() {\n\
               p := new(pool_stats)\n\
               p.processed = 5\n\
               fmt.println(\"{p.processed}\")\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    // -----------------------------------------------------------------------
    // Builtin `copy(dst, src)` shares reference element storage (#178). For a
    // slice whose element carries mutable GC storage, `copy` duplicates the
    // element *references*, so `dst` and `src` become may-share peers and a
    // later `move` of one must freeze the other (O5.2 / A1b). Regression for
    // the accepted-but-unsound shape where the source stayed live after
    // `copy` + `move dst`, racing under `go run -race` with 0 diagnostics.
    // -----------------------------------------------------------------------

    #[test]
    fn copy_reference_element_alias_rejects() {
        let r = check(
            "fn worker(d []map[string]int, done chan<- bool) {\n\
               d[0][\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               src := []map[string]int{map[string]int{}}\n\
               dst := make([]map[string]int, 1)\n\
               copy(dst, src)\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               src[0][\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn copy_primitive_element_accepts() {
        // A `[]int` copy duplicates values, not references — no shared GC
        // storage — so moving `dst` must NOT freeze `src` (no false positive).
        let r = check(
            "fn worker(d []int, done chan<- bool) {\n\
               d[0] = 9\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               src := []int{1}\n\
               dst := make([]int, 1)\n\
               copy(dst, src)\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               src[0] = 5\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn fc_atomic_pointer_and_value_reject_e3005_row_11() {
        let r_ptr = check(
            "import \"sync/atomic\"\n\
             struct holder { data map[string]int }\n\
             fn worker(p *atomic.pointer[holder], done chan<- bool) {\n\
               h := p.load()\n\
               for i in 0..10 { h.data[\"k\"] = i }\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               var p atomic.pointer[holder]\n\
               shared := holder{data: map[string]int{\"k\": 0}}\n\
               p.store(&shared)\n\
               done := make(chan bool, 1)\n\
               task worker(&p, done)\n\
               <-done\n\
             }",
        );
        assert_eq!(r_ptr.verdict, Verdict::Reject, "{:?}", r_ptr.diagnostics);
        assert_eq!(first_error_rule(&r_ptr).as_deref(), Some("FC"));
        let d = r_ptr
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("atomic.pointer[T] must fail closed with HYB-E3005");
        assert!(
            d.notes.iter().any(|n| n.contains("GUARANTEES.md row 11")),
            "notes must cite GUARANTEES.md row 11: {:?}",
            d.notes
        );

        let r_val = check(
            "import \"sync/atomic\"\n\
             fn worker(v *atomic.value) { }\n\
             fn main() {\n\
               var v atomic.value\n\
               task worker(&v)\n\
             }",
        );
        assert_eq!(r_val.verdict, Verdict::Reject, "{:?}", r_val.diagnostics);
        assert!(r_val.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn move_of_container_holding_closures_rejects() {
        // A closure that captures `m` is wrapped in a slice / map / struct and
        // the CONTAINER is moved to a task. `move v` froze only `v`, never the
        // captured `m`, so main kept writing `m` while the task wrote it
        // through the closure: ACCEPT with 0 diagnostics and a `go run -race`
        // hit before this fix (#225). Moving a bare closure was already
        // rejected.
        let shapes = [
            ("v []fn()", "v[0]()", "v := []fn(){fn() { m[\"k\"]++ }}", ""),
            (
                "v map[string]fn()",
                "v[\"a\"]()",
                "v := map[string]fn(){\"a\": fn() { m[\"k\"]++ }}",
                "",
            ),
            (
                "v job",
                "v.f()",
                "v := job{f: fn() { m[\"k\"]++ }}",
                "struct job { f fn() }\n",
            ),
            (
                "v outer",
                "v.j.f()",
                "v := outer{j: job{f: fn() { m[\"k\"]++ }}}",
                "struct job { f fn() }\nstruct outer { j job }\n",
            ),
        ];
        for (param, call, def, pre) in shapes {
            let src = format!(
                "{pre}fn worker({param}, done chan<- bool) {{\n  {call}\n  done <- true\n}}\n\
                 fn main() {{\n  m := map[string]int{{}}\n  {def}\n  done := make(chan bool, 1)\n  \
                 task worker(move v, done)\n  m[\"k\"] = 2\n  <-done\n}}\n"
            );
            let r = check(&src);
            assert_eq!(r.verdict, Verdict::Reject, "{param}: {:?}", r.diagnostics);
            assert!(
                r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
                "{param}: {:?}",
                r.diagnostics
            );
        }
        // Negative control: a moved container of plain values still crosses.
        let ok = check(
            "fn worker(v []int, done chan<- bool) {\n  v[0] = 1\n  done <- true\n}\n\
             fn main() {\n  v := []int{0}\n  done := make(chan bool, 1)\n  task worker(move v, done)\n  <-done\n}\n",
        );
        assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
    }

    #[test]
    fn non_owned_param_capture_and_range_moves_reject_e3004() {
        // 1. Function parameter moved across a task boundary: caller retains `m`.
        let r_param = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn spawn_it(m map[string]int, done chan<- bool) {\n\
               task worker(move m, done)\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               spawn_it(m, done)\n\
               m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r_param.verdict, Verdict::Reject, "{:?}", r_param.diagnostics);
        assert_eq!(first_error_rule(&r_param).as_deref(), Some("A1b"));
        assert!(r_param.diagnostics.iter().any(|d| d.code == "HYB-E3004"));

        // 2.Captured outer binding moved inside a closure: enclosing scope retains `m`.
        let r_cap = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 task worker(move m, done)\n\
               }\n\
               f()\n\
               m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r_cap.verdict, Verdict::Reject, "{:?}", r_cap.diagnostics);
        assert_eq!(first_error_rule(&r_cap).as_deref(), Some("A1b"));
        assert!(r_cap.diagnostics.iter().any(|d| d.code == "HYB-E3004"));

        // 3. Non-channel `for k, v in mm` range variable moved: `mm` still holds the element.
        let r_range = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               mm := map[string]map[string]int{\"a\": map[string]int{\"k\": 0}}\n\
               for k, v in mm {\n\
                 task worker(move v)\n\
               }\n\
               mm[\"a\"][\"k\"] = 2\n\
             }",
        );
        assert_eq!(r_range.verdict, Verdict::Reject, "{:?}", r_range.diagnostics);
        assert_eq!(first_error_rule(&r_range).as_deref(), Some("A1b"));
        assert!(r_range.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
        assert!(r_range.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 4. Positive control: channel iteration (`for b in ch`) transfers ownership per element.
        let r_chan = check(
            "struct batch { rows []int }\n\
             fn relay(in_ch <-chan batch, out_ch chan<- batch) {\n\
               for b in in_ch {\n\
                 out_ch <- move b\n\
               }\n\
             }",
        );
        assert_eq!(r_chan.verdict, Verdict::Accept, "{:?}", r_chan.diagnostics);
    }

    // -----------------------------------------------------------------------
    // Multi-value unpack (`n, m = make_pair()` / `n, m := make_pair()`),
    // parallel assignment aliases (`m2, x = m, 1`), and pointer-receiver
    // method aliases (`fn (s *state) run() { p := s; ... }`).
    // -----------------------------------------------------------------------

    #[test]
    fn multival_assign_does_not_overwrite_map_with_first_result_shape_e3001() {
        // Regression: `n, m = make_pair()` had `values.len() == 1` and
        // `targets.len() == 2`, and `process_assign` assigned
        // `shape_of_expr(&values[0])` (`f.results.first()` = Primitive) to
        // EVERY target, turning `m` (`map[string]int`) into a Primitive and
        // accepting `task worker(m, done)` with 0 diagnostics.
        let r = check(
            "fn make_pair() (int, map[string]int) {\n\
               return 1, map[string]int{\"k\": 0}\n\
             }\n\
             fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               var n int\n\
               var m map[string]int\n\
               n, m = make_pair()\n\
               done := make(chan bool, 1)\n\
               task worker(m, done)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("map[string]int"));
    }

    #[test]
    fn multival_define_resolves_declared_fn_results() {
        // `n, m := make_pair()` resolves each binding from its corresponding
        // declared function result: `n` (int) is sendable (Accept), while
        // `m` (map[string]int) is rejected under A1c (`HYB-E3001`).
        let r_reject = check(
            "fn make_pair() (int, map[string]int) {\n\
               return 1, map[string]int{\"k\": 0}\n\
             }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               n, m := make_pair()\n\
               fmt.println(n)\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(
            r_reject.verdict,
            Verdict::Reject,
            "{:?}",
            r_reject.diagnostics
        );
        assert_eq!(first_error_rule(&r_reject).as_deref(), Some("A1c"));
        assert_eq!(r_reject.diagnostics[0].code, "HYB-E3001");

        let r_accept = check(
            "fn make_pair() (int, string) {\n\
               return 1, \"ok\"\n\
             }\n\
             fn worker(n int, s string) { fmt.println(n, s) }\n\
             fn main() {\n\
               n, s := make_pair()\n\
               task worker(n, s)\n\
             }",
        );
        assert_eq!(
            r_accept.verdict,
            Verdict::Accept,
            "{:?}",
            r_accept.diagnostics
        );
    }

    #[test]
    fn parallel_assign_records_reference_alias_for_freeze_e3003() {
        // Regression: `m2, x = m, 1` had `values.len() == 2` and skipped the
        // `values.len() == 1` guard in `process_assign`, so `m2 -> m` was
        // never recorded in `st.aliases` and `m2["k"] = 2` after `move m`
        // passed with 0 diagnostics.
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               var m2 map[string]int\n\
               var x int\n\
               m2, x = m, 1\n\
               done := make(chan bool, 1)\n\
               task worker(move m, done)\n\
               m2[\"k\"] = 2\n\
               fmt.println(x)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn pointer_receiver_method_preserves_pointer_shape_and_freezes_alias_e3003() {
        // Regression: `parse_receiver` strips `*` into `r.is_pointer = true`
        // and stores the base type in `r.ty`, but `check_fn` ignored
        // `r.is_pointer` and gave `s` `TypeShape::Struct` instead of
        // `TypeShape::Pointer(Struct)`. As a result, `p := s` inside
        // `fn (s *state) run(...)` was not recorded as a reference alias and
        // `p.m["k"] = 2` after `task worker(move s, done)` passed with 0
        // diagnostics.
        let r = check(
            "struct state { m map[string]int }\n\
             fn worker(s *state, done chan<- bool) {\n\
               s.m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn (s *state) run(done chan<- bool) {\n\
               p := s\n\
               task worker(move s, done)\n\
               p.m[\"k\"] = 2\n\
             }\n\
             fn main() {\n\
               st := state{m: map[string]int{\"k\": 0}}\n\
               done := make(chan bool, 1)\n\
               st.run(done)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        // The E3004 (receiver `s` is not exclusively owned, so `move s` is
        // invalid outright — the non-owned-move guard) now fires at the move
        // site BEFORE the alias story can; the alias freeze (E3003) remains
        // the diagnosis for the owned-binding variant of this shape.
        assert_eq!(r.diagnostics[0].code, "HYB-E3004");
    }

    /// Generalized alias collector (#160): a reference-typed value that
    /// reaches a binding through anything other than a bare ident — a nested
    /// call, a view of a call result, a composite literal holding a call
    /// result, a field read out of a literal, a call returning a parameter
    /// field, or a plain assignment — must still be recorded as an alias so
    /// that a later `move` of either side freezes the other (O5.2 / A1b).
    /// Every shape below was accepted with 0 diagnostics and raced under
    /// `go run -race` before `collect_ref_roots` replaced the bare-ident-only
    /// `Expr::root_ident()` walk.
    fn assert_alias_reject(src: &str) {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn nested_call_result_alias_freezes_e3003() {
        assert_alias_reject(
            "fn same(s []int) []int { return s }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               t := same(same(s))\n\
               task worker(move t)\n\
               s[0] = 7\n\
             }",
        );
    }

    #[test]
    fn view_of_call_result_alias_freezes_e3003() {
        assert_alias_reject(
            "fn same(s []int) []int { return s }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               t := same(s)[0:2]\n\
               task worker(move t)\n\
               s[0] = 7\n\
             }",
        );
    }

    #[test]
    fn composite_literal_holding_call_result_freezes_e3003() {
        assert_alias_reject(
            "struct wrap { items []int }\n\
             fn same(s []int) []int { return s }\n\
             fn worker(w wrap) { w.items[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               w := wrap{items: same(s)}\n\
               task worker(move w)\n\
               s[0] = 7\n\
             }",
        );
    }

    #[test]
    fn field_of_composite_literal_alias_freezes_e3003() {
        assert_alias_reject(
            "struct box { items []int }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               t := box{items: s}.items\n\
               task worker(move t)\n\
               s[0] = 7\n\
             }",
        );
    }

    #[test]
    fn call_returning_parameter_field_alias_freezes_e3003() {
        assert_alias_reject(
            "struct box { items []int }\n\
             fn take(b box) []int { return b.items }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               t := take(box{items: s})\n\
               task worker(move t)\n\
               s[0] = 7\n\
             }",
        );
    }

    #[test]
    fn assigned_call_result_alias_freezes_e3003() {
        assert_alias_reject(
            "fn same(s []int) []int { return s }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               var t []int\n\
               t = same(s)\n\
               task worker(move t)\n\
               s[0] = 7\n\
             }",
        );
    }

    /// Positive control: a call with no reference-carrying argument is a
    /// fresh value and must NOT be edged to anything — the widening must not
    /// start rejecting fresh results.
    #[test]
    fn fresh_call_result_stays_accepted() {
        let r = check(
            "fn fresh() []int { return []int{1, 2} }\n\
             fn worker(s []int) { s[0] = 1 }\n\
             fn main() {\n\
               t := fresh()\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    /// Positive control: `len(m)` is a scalar copy-out, not a reference
    /// alias, so moving the result must not freeze the map argument.
    #[test]
    fn scalar_copy_out_is_not_an_alias() {
        let r = check(
            "fn worker(n int) { fmt.println(n) }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               n := len(m)\n\
               task worker(move n)\n\
               m[\"k\"] = 1\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    /// C5: a select send clause (`ch <- v:`) is a sendability check-site
    /// "exactly like statement sends", so the class-3 owned-move route
    /// (`ch <- move v`) must work there too.
    ///
    /// The parser's `is_move` marker was dropped when a send statement was
    /// converted into a select arm (`SelectArmKind::Send` carried no flag),
    /// and both check sites passed a hardcoded `false` as `implicit_move`.
    /// Consequences: `select { out <- move m: {} }` was rejected A1c
    /// (HYB-E3002) even though the statement-send twin `out <- move m` was
    /// accepted, and — the half that matters for the guarantee — the move
    /// was never recorded, so the O5.2 freeze on `m` never began and the
    /// sender could keep writing the value it had just handed away.
    #[test]
    fn select_send_move_marker_is_honored_and_freezes_the_source() {
        // 1. The repro: a legal owned move-send inside a select arm.
        let r = check(
            "fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               out := make(chan map[string]int, 1)\n\
               select {\n\
                 out <- move m: {}\n\
               }\n\
               fmt.println(\"sent\")\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);

        // 2. Statement-send control: the same route already worked there.
        let r_stmt = check(
            "fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               out := make(chan map[string]int, 1)\n\
               out <- move m\n\
               fmt.println(\"sent\")\n\
             }",
        );
        assert_eq!(r_stmt.verdict, Verdict::Accept, "{:?}", r_stmt.diagnostics);

        // 3. Negative control: WITHOUT `move` the same arm still rejects.
        //    The fix must not widen the sendability rule.
        let r_plain = check(
            "fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               out := make(chan map[string]int, 1)\n\
               select {\n\
                 out <- m: {}\n\
               }\n\
             }",
        );
        assert_eq!(r_plain.verdict, Verdict::Reject, "{:?}", r_plain.diagnostics);
        assert!(r_plain.diagnostics.iter().any(|d| d.code == "HYB-E3002"));

        // 4. The freeze now begins at the select-send: the sender's later
        //    write is A1b / HYB-E3003, exactly as after a statement send.
        let r_freeze = check(
            "fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               out := make(chan map[string]int, 1)\n\
               select {\n\
                 out <- move m: {}\n\
               }\n\
               m[\"k\"] = 1\n\
             }",
        );
        assert_eq!(r_freeze.verdict, Verdict::Reject, "{:?}", r_freeze.diagnostics);
        assert_eq!(first_error_rule(&r_freeze).as_deref(), Some("A1b"));
        assert!(r_freeze.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 5. The non-ident form keeps its own rule: `move h.m` in a select
        //    arm is O5.4 / HYB-E3004, same as the statement-send spelling.
        let r_field = check(
            "struct holder { m map[string]int }\n\
             fn main() {\n\
               h := holder{m: map[string]int{\"k\": 0}}\n\
               out := make(chan map[string]int, 1)\n\
               select {\n\
                 out <- move h.m: {}\n\
               }\n\
             }",
        );
        assert_eq!(r_field.verdict, Verdict::Reject, "{:?}", r_field.diagnostics);
        assert!(
            r_field.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r_field.diagnostics
        );
    }

    #[test]
    fn named_result_bindings_and_bare_return_soundness() {
        // 1. Named result `(out map[string]int)` shadowing a file-level
        // `const out = 0` must classify as a shared map at `task worker(out)`
        // (HYB-E3001), never fall back to the primitive file const.
        let r1 = check(
            "const out = 0\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn run() (out map[string]int) {\n\
               out = map[string]int{\"k\": 0}\n\
               task worker(out)\n\
               out[\"k\"] = 2\n\
               return out\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3001"));

        // 2. Direct move `task worker(move out)` freezes `out` and rejects
        // subsequent writes (`out["k"] = 2`) and bare `return` (HYB-E3003).
        let r2 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn run() (out map[string]int) {\n\
               out = map[string]int{\"k\": 0}\n\
               task worker(move out)\n\
               out[\"k\"] = 2\n\
               return\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 3. Alias move `out = m; task worker(move m); return` rejects bare
        // `return` of the frozen named result alias `out` (HYB-E3003).
        let r3 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn run() (out map[string]int) {\n\
               m := map[string]int{\"k\": 0}\n\
               out = m\n\
               task worker(move m)\n\
               return\n\
             }",
        );
        assert_eq!(r3.verdict, Verdict::Reject, "{:?}", r3.diagnostics);
        assert!(r3.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 4. Assigning a parameter `p` to a named result `out = p` preserves
        // `non_owned_origin` and rejects `task worker(move out)` with HYB-E3004.
        let r4 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn run(p map[string]int) (out map[string]int) {\n\
               out = p\n\
               task worker(move out)\n\
               return map[string]int{\"k\": 0}\n\
             }",
        );
        assert_eq!(r4.verdict, Verdict::Reject, "{:?}", r4.diagnostics);
        assert!(r4.diagnostics.iter().any(|d| d.code == "HYB-E3004"));

        // 5. Bare `return` of a named result holding `&m[1]` extends the A3
        // slot-loan window to `return` (HYB-E3102 on `m[2] = 20`) and rejects
        // the escaping borrow with HYB-E3104.
        let r5 = check(
            "type int_ptr = *int\n\
             fn bad() (out int_ptr) {\n\
               m := map[int]int{1: 10}\n\
               out = &m[1]\n\
               m[2] = 20\n\
               return\n\
             }",
        );
        assert_eq!(r5.verdict, Verdict::Reject, "{:?}", r5.diagnostics);
        assert!(r5.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
        assert!(r5.diagnostics.iter().any(|d| d.code == "HYB-E3104"));

        // 6. Closure with named result `(out map[string]int)` tracks `out` at
        // boundaries (HYB-E3001 on unmoved spawn, HYB-E3003 on post-move bare return).
        let r6 = check(
            "const out = 0\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               f := fn() (out map[string]int) {\n\
                 out = map[string]int{\"k\": 0}\n\
                 task worker(out)\n\
                 return\n\
               }\n\
               _ = f()\n\
             }",
        );
        assert_eq!(r6.verdict, Verdict::Reject, "{:?}", r6.diagnostics);
        assert!(r6.diagnostics.iter().any(|d| d.code == "HYB-E3001"));

        // 7. Assigning `nil` (`HYB-E4001`) or receiving an incompatible
        // channel element (`HYB-E3011`) into a named result binding rejects.
        let r7_nil = check(
            "fn bad_nil() (out map[string]int) {\n\
               out = nil\n\
               return\n\
             }",
        );
        assert_eq!(r7_nil.verdict, Verdict::Reject, "{:?}", r7_nil.diagnostics);
        assert!(r7_nil.diagnostics.iter().any(|d| d.code == "HYB-E4001"));

        let r7_chan = check(
            "fn bad_recv(ch chan string) (out int) {\n\
               out = <-ch\n\
               return\n\
             }",
        );
        assert_eq!(r7_chan.verdict, Verdict::Reject, "{:?}", r7_chan.diagnostics);
        assert!(r7_chan.diagnostics.iter().any(|d| d.code == "HYB-E3011"));

        // 8. Positive controls: unmoved named results with bare `return` stay accepted.
        let r8 = check(
            "fn ok() (out map[string]int) {\n\
               out = map[string]int{\"k\": 1}\n\
               return\n\
             }",
        );
        assert_eq!(r8.verdict, Verdict::Accept, "{:?}", r8.diagnostics);
    }

    // -----------------------------------------------------------------------
    // `if <init>; <cond>` evaluation order (Go evaluates `<init>` before
    // `<cond>`) and same-statement consume / package-level boundary calls.
    // -----------------------------------------------------------------------

    #[test]
    fn if_init_eval_order_enforces_a1b_a7_a2_a3_and_t22_in_cond() {
        // 1. A1b: moving `m` in `if <init>` freezes `m` before `<cond>` runs.
        let r_a1b = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 2\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 1}\n\
               done := make(chan bool, 1)\n\
               if m2 := move m; m[\"k\"] == 1 {\n\
                 task worker(move m2, done)\n\
               }\n\
             }",
        );
        assert_eq!(r_a1b.verdict, Verdict::Reject, "{:?}", r_a1b.diagnostics);
        assert_eq!(first_error_rule(&r_a1b).as_deref(), Some("A1b"));
        assert!(r_a1b.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 2. A1b inside `else if <init>; <cond>` and inside a closure body.
        let r_closure_if = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 2\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 m := map[string]int{\"k\": 1}\n\
                 if false {\n\
                 } else if m2 := move m; m[\"k\"] == 1 {\n\
                   task worker(move m2, done)\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(
            r_closure_if.verdict,
            Verdict::Reject,
            "{:?}",
            r_closure_if.diagnostics
        );
        assert_eq!(first_error_rule(&r_closure_if).as_deref(), Some("A1b"));
        assert!(r_closure_if
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 3. A7: consuming `enc.finish()?` in `if <init>` freezes `enc` before `<cond>`.
        let r_a7 = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               if out := enc.finish()?; enc.write([]byte(\"late\"))? > 0 {\n\
                 fmt.println(len(out))\n\
               }\n\
             }",
        );
        assert_eq!(r_a7.verdict, Verdict::Reject, "{:?}", r_a7.diagnostics);
        assert_eq!(first_error_rule(&r_a7).as_deref(), Some("A7"));
        assert!(r_a7.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        // 4. A2: borrow handle opened in `if <init>` conflicts with mutation in `<cond>`
        // both when the view is used in the body and when its last use is inside `<cond>`.
        let r_a2_body = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               if u := doc.array(\"users\")?; doc.set(\"pruned\", true)? == 0 {\n\
                 fmt.println(u.int(\"n\")?)\n\
               }\n\
             }",
        );
        assert_eq!(
            r_a2_body.verdict,
            Verdict::Reject,
            "{:?}",
            r_a2_body.diagnostics
        );
        assert_eq!(first_error_rule(&r_a2_body).as_deref(), Some("A2"));
        assert!(r_a2_body.diagnostics.iter().any(|d| d.code == "HYB-E3101"));

        let r_a2_cond = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               if u := doc.array(\"users\")?; doc.set(\"x\", u.int(\"n\")?)? == 0 {\n\
                 fmt.println(\"done\")\n\
               }\n\
             }",
        );
        assert_eq!(
            r_a2_cond.verdict,
            Verdict::Reject,
            "{:?}",
            r_a2_cond.diagnostics
        );
        assert_eq!(first_error_rule(&r_a2_cond).as_deref(), Some("A2"));
        assert!(r_a2_cond.diagnostics.iter().any(|d| d.code == "HYB-E3101"));

        // 5. A3: `&e.m["a"]` loan opened in `if <init>` or mutated via a closure
        // defined in `if <init>` and called in `<cond>`.
        let r_a3_cond = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn mut_and_read(m map[string]svc, v int) bool {\n\
               m[\"b\"] = svc{n: v}\n\
               return true\n\
             }\n\
             fn f(e *env) {\n\
               if s := &e.m[\"a\"]; mut_and_read(e.m, s.n) {\n\
                 fmt.println(\"ok\")\n\
               }\n\
             }",
        );
        assert_eq!(
            r_a3_cond.verdict,
            Verdict::Reject,
            "{:?}",
            r_a3_cond.diagnostics
        );
        assert_eq!(first_error_rule(&r_a3_cond).as_deref(), Some("A3"));
        assert!(r_a3_cond.diagnostics.iter().any(|d| d.code == "HYB-E3102"));

        let r_a3_closure = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               s := &e.m[\"a\"]\n\
               if bump := fn() bool { e.m[\"b\"] = svc{n: 1}\n return true }; bump() {\n\
                 s.n = 2\n\
               }\n\
             }",
        );
        assert_eq!(
            r_a3_closure.verdict,
            Verdict::Reject,
            "{:?}",
            r_a3_closure.diagnostics
        );
        assert_eq!(first_error_rule(&r_a3_closure).as_deref(), Some("A3"));
        assert!(r_a3_closure
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3102"));

        // 6. T22: const alias bound in `if <init>` cannot be mutated via `delete` in `<cond>`.
        let r_t22 = check(
            "const c = map[string]int{\"k\": 1}\n\
             fn is_ok(x any) bool { return true }\n\
             fn main() {\n\
               if alias := c; is_ok(delete(alias, \"k\")) {\n\
                 fmt.println(\"deleted\")\n\
               }\n\
             }",
        );
        assert_eq!(r_t22.verdict, Verdict::Reject, "{:?}", r_t22.diagnostics);
        assert_eq!(first_error_rule(&r_t22).as_deref(), Some("IMMUT"));
        assert!(r_t22.diagnostics.iter().any(|d| d.code == "HYB-E3006"));

        // 7. Positive control: using the new binding `m2` in `<cond>` after `if m2 := move m`
        // and mutating `doc` after a view opened in `if <init>` dies in `<cond>` stay legal.
        let r_ok = check(
            "import \"rust/serde_json\"\n\
             fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 2\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 1}\n\
               done := make(chan bool, 1)\n\
               if m2 := move m; m2[\"k\"] == 1 {\n\
                 task worker(move m2, done)\n\
               }\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               if u := doc.array(\"users\")?; u.int(\"n\")? == 0 {\n\
                 doc.set(\"pruned\", true)?\n\
               }\n\
             }",
        );
        assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
    }

    #[test]
    fn a7_same_stmt_use_with_consuming_call_and_package_handle_calls_reject() {
        // 1. Same-statement consume + mutate (`a, b := enc.finish()?, enc.write(...)?`).
        let r_consume_mutate = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               a, b := enc.finish()?, enc.write([]byte(\"x\"))?\n\
               fmt.println(len(a), b)\n\
             }",
        );
        assert_eq!(
            r_consume_mutate.verdict,
            Verdict::Reject,
            "{:?}",
            r_consume_mutate.diagnostics
        );
        assert_eq!(first_error_rule(&r_consume_mutate).as_deref(), Some("A7"));
        assert!(r_consume_mutate
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 2. Same-statement double consume (`a, b := enc.finish()?, enc.finish()?`).
        let r_double_consume = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               a, b := enc.finish()?, enc.finish()?\n\
               fmt.println(len(a), len(b))\n\
             }",
        );
        assert_eq!(
            r_double_consume.verdict,
            Verdict::Reject,
            "{:?}",
            r_double_consume.diagnostics
        );
        assert_eq!(first_error_rule(&r_double_consume).as_deref(), Some("A7"));
        assert!(r_double_consume
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 3. Nested consume inside receiver call (`enc.write(enc.finish()?)?`).
        let r_nested_consume = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               enc.write(enc.finish()?)?\n\
             }",
        );
        assert_eq!(
            r_nested_consume.verdict,
            Verdict::Reject,
            "{:?}",
            r_nested_consume.diagnostics
        );
        assert_eq!(first_error_rule(&r_nested_consume).as_deref(), Some("A7"));
        assert!(r_nested_consume
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 4. Package-form consuming call (`flate2.finish(enc)?`) freezes `enc` under A7.
        let r_pkg_consume = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               out := flate2.finish(enc)?\n\
               flate2.write(enc, []byte(\"trailer\"))?\n\
               fmt.println(len(out))\n\
             }",
        );
        assert_eq!(
            r_pkg_consume.verdict,
            Verdict::Reject,
            "{:?}",
            r_pkg_consume.diagnostics
        );
        assert_eq!(first_error_rule(&r_pkg_consume).as_deref(), Some("A7"));
        assert!(r_pkg_consume
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 5. Package-form borrow + mutate (`serde_json.array(doc, ...)` + `serde_json.set(doc, ...)`)
        // enforces A2 (`HYB-E3101`) both in `for .. in` and across local view bindings.
        let r_pkg_a2_loop = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               for u in serde_json.array(doc, \"users\")? {\n\
                 if serde_json.int(u, \"logins\")? == 0 {\n\
                   serde_json.set(doc, \"pruned\", true)?\n\
                 }\n\
               }\n\
             }",
        );
        assert_eq!(
            r_pkg_a2_loop.verdict,
            Verdict::Reject,
            "{:?}",
            r_pkg_a2_loop.diagnostics
        );
        assert_eq!(first_error_rule(&r_pkg_a2_loop).as_deref(), Some("A2"));
        assert!(r_pkg_a2_loop
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3101"));

        let r_pkg_a2_view = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               u := serde_json.get(doc, \"users\")?\n\
               serde_json.remove(doc, \"users\")?\n\
               fmt.println(serde_json.int(u, \"n\")?)\n\
             }",
        );
        assert_eq!(
            r_pkg_a2_view.verdict,
            Verdict::Reject,
            "{:?}",
            r_pkg_a2_view.diagnostics
        );
        assert_eq!(first_error_rule(&r_pkg_a2_view).as_deref(), Some("A2"));
        assert!(r_pkg_a2_view
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3101"));

        // 6. Positive control: sequential `flate2.write(enc, ...)` before `flate2.finish(enc)`
        // and `serde_json.set(doc, ...)` after `serde_json.array(doc, ...)` loan ends stay legal.
        let r_pkg_ok = check(
            "import \"rust/flate2\"\n\
             import \"rust/serde_json\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               flate2.write(enc, []byte(\"ok\"))?\n\
               out := flate2.finish(enc)?\n\
               doc := serde_json.parse(`{\"users\": []}`)?\n\
               for u in serde_json.array(doc, \"users\")? {\n\
                 fmt.println(serde_json.int(u, \"logins\")?)\n\
               }\n\
               serde_json.set(doc, \"pruned\", true)?\n\
               fmt.println(len(out))\n\
             }",
        );
        assert_eq!(r_pkg_ok.verdict, Verdict::Accept, "{:?}", r_pkg_ok.diagnostics);
    }

    #[test]
    fn e3001_e3002_fixits_name_managed_reason_and_category_syntax() {
        // Track #7 wording sweep: HYB-E3001 and HYB-E3002 fix-its must name the
        // required `managed("why this bypass is safe", "category") { ... }` form
        // and the closed category list (matching HYB-E3005 after PR #11) so
        // following the fix-it does not hit HYB-E0003.
        let r_spawn = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               task worker(m)\n\
             }",
        );
        let e3001 = r_spawn
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3001")
            .expect("HYB-E3001 at spawn");
        assert!(
            e3001.fixits.iter().any(|f| {
                f.description
                    .contains("managed(\"why this bypass is safe\", \"category\")")
                    && f.description
                        .contains("mutex, atomic, actor, finalizer, interop, other")
            }),
            "HYB-E3001 fixits must name managed(reason, category) and category set: {:?}",
            e3001.fixits
        );

        let r_send = check(
            "fn main() {\n\
               ch := make(chan map[string]int, 1)\n\
               m := map[string]int{\"k\": 0}\n\
               ch <- m\n\
             }",
        );
        let e3002 = r_send
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3002")
            .expect("HYB-E3002 at send");
        assert!(
            e3002.fixits.iter().any(|f| {
                f.description
                    .contains("managed(\"why this bypass is safe\", \"category\")")
                    && f.description
                        .contains("mutex, atomic, actor, finalizer, interop, other")
            }),
            "HYB-E3002 fixits must name managed(reason, category) and category set: {:?}",
            e3002.fixits
        );
    }

    #[test]
    fn move_of_unproven_atomic_cites_guarantees_row_11_not_missing_ascription() {
        // Track #7 wording sweep: `classify_ident` and `classify_addr_of`
        // consulted `unproven_atomic_reason(b)`, but `classify_move_ident` did
        // not — so `task worker(move p)` on an explicitly ascribed
        // `var p atomic.pointer[int]` falsely claimed "no ascription" instead
        // of explaining GUARANTEES.md row 11.
        let r = check(
            "fn worker(p atomic.pointer[int]) {}\n\
             fn main() {\n\
               var p atomic.pointer[int]\n\
               task worker(move p)\n\
             }",
        );
        let e3005 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("HYB-E3005 on move of non-scalar atomic");
        assert!(
            e3005
                .notes
                .iter()
                .any(|n| n.contains("GUARANTEES.md row 11") && n.contains("atomic.pointer[int]")),
            "move of non-scalar atomic must cite GUARANTEES.md row 11: {:?}",
            e3005.notes
        );
        assert!(
            !e3005.notes.iter().any(|n| n.contains("no ascription")),
            "must not claim 'no ascription' when `var p atomic.pointer[int]` is ascribed: {:?}",
            e3005.notes
        );
    }

    // -----------------------------------------------------------------------
    // Issue #143: T13/T14/T15 (`HYB-E5001`) match exhaustiveness & `_`
    // placement + N3 (`HYB-E4002`) enum/struct zero-value checks (R1–R6).
    // -----------------------------------------------------------------------

    #[test]
    fn match_exhaustiveness_and_wildcard_placement_reject_e5001() {
        // R1: option match with only `none` arm rejects with HYB-E5001
        let r1 = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 none: fmt.println(\"none\")\n\
               }\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert_eq!(r1.diagnostics[0].code, "HYB-E5001");
        assert!(r1.diagnostics[0].message.contains("some(v)"));

        // R2: option match with only `some(v)` arm rejects with HYB-E5001
        let r2 = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 some(v): fmt.println(v)\n\
               }\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert_eq!(r2.diagnostics[0].code, "HYB-E5001");
        assert!(r2.diagnostics[0].message.contains("none"));

        // R3: enum match missing variants rejects with HYB-E5001 naming missing variants
        let r3 = check(
            "enum shape {\n\
               circle(f64)\n\
               square(f64)\n\
               point\n\
             }\n\
             fn area(s shape) {\n\
               match s {\n\
                 circle(r): fmt.println(r)\n\
               }\n\
             }\n\
             fn main() {}",
        );
        assert_eq!(r3.verdict, Verdict::Reject, "{:?}", r3.diagnostics);
        assert_eq!(r3.diagnostics[0].code, "HYB-E5001");
        assert!(r3.diagnostics[0].message.contains("square"));
        assert!(r3.diagnostics[0].message.contains("point"));

        // R4: `_` not last (or only arm) rejects with HYB-E5001
        let r4 = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 _: fmt.println(\"wild\")\n\
                 some(v): fmt.println(v)\n\
               }\n\
             }",
        );
        assert_eq!(r4.verdict, Verdict::Reject, "{:?}", r4.diagnostics);
        assert_eq!(r4.diagnostics[0].code, "HYB-E5001");

        // Positive controls: exhaustive option match (`some(v)` + `none`, or `some(v)` + `_`)
        // and exhaustive enum match accept with 0 diagnostics.
        let r_ok_opt = check(
            "fn main() {\n\
               var x option[int] = some(1)\n\
               match x {\n\
                 some(v): fmt.println(v)\n\
                 _: fmt.println(\"none\")\n\
               }\n\
             }",
        );
        assert_eq!(r_ok_opt.verdict, Verdict::Accept, "{:?}", r_ok_opt.diagnostics);

        let r_ok_enum = check(
            "enum shape {\n\
               point\n\
               circle(f64)\n\
               square(f64)\n\
             }\n\
             fn area(s shape) {\n\
               match s {\n\
                 point: fmt.println(0)\n\
                 circle(r): fmt.println(r)\n\
                 square(w): fmt.println(w)\n\
               }\n\
             }\n\
             fn main() {}",
        );
        assert_eq!(r_ok_enum.verdict, Verdict::Accept, "{:?}", r_ok_enum.diagnostics);
    }

    #[test]
    fn enum_and_struct_without_zero_value_reject_e4002() {
        // R5: enum whose first variant carries a payload has no zero value (N3 -> HYB-E4002)
        let r5 = check(
            "enum shape {\n\
               circle(f64)\n\
               point\n\
             }\n\
             fn main() {\n\
               var s shape\n\
               fmt.println(s)\n\
             }",
        );
        assert_eq!(r5.verdict, Verdict::Reject, "{:?}", r5.diagnostics);
        assert_eq!(r5.diagnostics[0].code, "HYB-E4002");

        // R6: struct containing a field whose type has no zero value also rejects (N3 -> HYB-E4002)
        let r6 = check(
            "enum shape { circle(f64) }\n\
             struct box { s shape }\n\
             fn main() {\n\
               var b box\n\
               fmt.println(b)\n\
             }",
        );
        assert_eq!(r6.verdict, Verdict::Reject, "{:?}", r6.diagnostics);
        assert_eq!(r6.diagnostics[0].code, "HYB-E4002");

        // Positive control: enum whose first variant is payload-free HAS a zero value.
        let r_ok = check(
            "enum shape {\n\
               point\n\
               circle(f64)\n\
             }\n\
             struct box { s shape }\n\
             fn main() {\n\
               var s shape\n\
               var b box\n\
               fmt.println(s, b)\n\
             }",
        );
        assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
    }

#[test]
    fn closure_ascribed_local_assign_rebind_and_self_shadow_reject() {
        // 1. Type-ascribed local inside a closure (`var alias map[string]int = m`)
        // preserves `non_owned_origin` and rejects `move alias` with HYB-E3004.
        let r_ascribed_capture = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 var alias map[string]int = m\n\
                 task worker(move alias, done)\n\
               }\n\
               f()\n\
               <-done\n\
             }",
        );
        assert_eq!(r_ascribed_capture.verdict, Verdict::Reject);
        assert!(r_ascribed_capture
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004"));

        // 2. Assignment rebind inside a closure (`alias = m`) inherits
        // `non_owned_origin` and rejects `move alias` with HYB-E3004.
        let r_assign_capture = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 var alias map[string]int\n\
                 alias = m\n\
                 task worker(move alias, done)\n\
               }\n\
               f()\n\
               <-done\n\
             }",
        );
        assert_eq!(r_assign_capture.verdict, Verdict::Reject);
        assert!(r_assign_capture
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004"));

        // 3. Closure-owned map aliased via ascribed define (`var m2 map[string]int = m1`)
        // or assignment (`m2 = m1`) freezes when `m1` is moved (HYB-E3003).
        let r_closure_freeze = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 m1 := map[string]int{\"k\": 0}\n\
                 var m2 map[string]int\n\
                 m2 = m1\n\
                 task worker(move m1, done)\n\
                 m2[\"k\"] = 2\n\
               }\n\
               f()\n\
               <-done\n\
             }",
        );
        assert_eq!(r_closure_freeze.verdict, Verdict::Reject);
        assert!(r_closure_freeze
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 4. Self-shadowing define (`m := m`) inside a closure preserves the
        // captured binding's `non_owned_origin` and rejects `move m` (HYB-E3004).
        let r_self_shadow = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               f := fn() {\n\
                 m := m\n\
                 task worker(move m, done)\n\
               }\n\
               f()\n\
               <-done\n\
             }",
        );
        assert_eq!(r_self_shadow.verdict, Verdict::Reject);
        assert!(r_self_shadow
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004"));
    }

#[test]
    fn self_rebind_assign_and_param_peer_rebind_preserve_origin_and_shares() {
        // 1. Self-rebind assignment (`s = s[:]`, `m = same(m)`) on a parameter
        // preserves `non_owned_origin` and rejects `move` with HYB-E3004.
        let r_param_reslice = check(
            "fn worker(s []int, done chan<- bool) {\n\
               s[0] = 1\n\
               done <- true\n\
             }\n\
             fn spawn_it(s []int, done chan<- bool) {\n\
               s = s[:]\n\
               task worker(move s, done)\n\
             }\n\
             fn main() {\n\
               s := []int{0, 1}\n\
               done := make(chan bool, 1)\n\
               spawn_it(s, done)\n\
               s[0] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r_param_reslice.verdict, Verdict::Reject);
        assert!(r_param_reslice
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004"));

        // 2. Rebinding a struct parameter `h` after `h2 := h` transfers `h`'s
        // non-owned origin to `h2`, so `move h2` still rejects with HYB-E3004.
        let r_peer_rebind = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder, done chan<- bool) {\n\
               h.m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn launder(h holder, done chan<- bool) {\n\
               h2 := h\n\
               h = holder{m: map[string]int{\"fresh\": 0}}\n\
               task worker(move h2, done)\n\
             }\n\
             fn main() {\n\
               h := holder{m: map[string]int{\"k\": 0}}\n\
               done := make(chan bool, 1)\n\
               launder(h, done)\n\
               h.m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r_peer_rebind.verdict, Verdict::Reject);
        assert!(r_peer_rebind
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004" && d.message.contains("may-share peer")));

        // 3. Self-rebind of an owned map (`m1 = same(m1)`) preserves its
        // existing `may_share` edge to `h := holder{m: m1}`, so `move m1`
        // freezes `h` (HYB-E3003).
        let r_self_rebind_share = check(
            "struct holder { m map[string]int }\n\
             fn same(m map[string]int) map[string]int { return m }\n\
             fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m1 := map[string]int{\"k\": 0}\n\
               h := holder{m: m1}\n\
               m1 = same(m1)\n\
               done := make(chan bool, 1)\n\
               task worker(move m1, done)\n\
               h.m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(r_self_rebind_share.verdict, Verdict::Reject);
        assert!(r_self_rebind_share
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 4. Positive control: rebinding a parameter to a genuinely fresh map
        // (`m = map[string]int{"fresh": 0}`) re-owns `m` and stays accepted.
        let r_fresh_rebind = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn spawn_fresh(m map[string]int, done chan<- bool) {\n\
               m = map[string]int{\"fresh\": 0}\n\
               task worker(move m, done)\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               spawn_fresh(m, done)\n\
               m[\"k\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_fresh_rebind.verdict,
            Verdict::Accept,
            "{:?}",
            r_fresh_rebind.diagnostics
        );
    }

    #[test]
    fn shadow_rebind_and_expr_move_soundness_regressions() {
        // 1. Shadowing an intermediate alias/peer of a parameter (`a := p; h := holder{m: a}; if true { a := ...; move h }`)
        // preserves `p`'s non-owned origin on `h` -> HYB-E3004.
        let r_shadow_param_peer = check(
            "struct holder { m map[string]int }\n\
             fn worker(h holder, done chan<- bool) {\n\
               h.m[\"k\"] = 99\n\
               done <- true\n\
             }\n\
             fn leak(p map[string]int, done chan<- bool) {\n\
               a := p\n\
               h := holder{m: a}\n\
               if true {\n\
                 a := map[string]int{\"k\": 1}\n\
                 _ = a\n\
                 task worker(move h, done)\n\
               }\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 1}\n\
               done := make(chan bool, 1)\n\
               leak(m, done)\n\
               <-done\n\
             }",
        );
        assert_eq!(r_shadow_param_peer.verdict, Verdict::Reject);
        assert!(r_shadow_param_peer
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3004"));

        // 2. Shadowing an intermediate alias/peer of a `const` (`a := c; if true { h := holder{m: a}; a := ...; h.m["k"] = 99 }`)
        // preserves `const c`'s deep-immutability root on `h` -> HYB-E3006.
        let r_shadow_const_peer = check(
            "struct holder { m map[string]int }\n\
             const c = map[string]int{\"k\": 1}\n\
             fn main() {\n\
               a := c\n\
               if true {\n\
                 h := holder{m: a}\n\
                 a := map[string]int{\"k\": 2}\n\
                 _ = a\n\
                 h.m[\"k\"] = 99\n\
                 delete(h.m, \"k\")\n\
               }\n\
             }",
        );
        assert_eq!(r_shadow_const_peer.verdict, Verdict::Reject);
        assert!(r_shadow_const_peer
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3006"));

        // 3. Rebinding an intermediate bridge peer (`a <-> b <-> h`, `b = fresh`, `move a`)
        // preserves transitive sharing between `a` and `h` -> HYB-E3003.
        let r_rebind_bridge = check(
            "import \"fmt\"\n\
             struct holder { m map[string]int }\n\
             fn main() {\n\
               a := map[string]int{\"k\": 1}\n\
               b := a\n\
               h := holder{m: b}\n\
               b = map[string]int{\"k\": 2}\n\
               moved := move a\n\
               _ = moved\n\
               fmt.println(h.m[\"k\"])\n\
             }",
        );
        assert_eq!(r_rebind_bridge.verdict, Verdict::Reject);
        assert!(r_rebind_bridge
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 4. Moving an inner peer of a shadowed outer binding freezes the outer
        // binding after the shadow scope exits (even if the inner peer is later
        // rebound before block exit) -> HYB-E3003.
        let r_inner_peer_shadow_move = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               if true {\n\
                 m2 := m\n\
                 m := map[string]int{\"fresh\": 2}\n\
                 _ = m\n\
                 task worker(move m2, done)\n\
                 m2 = map[string]int{\"after\": 3}\n\
                 _ = m2\n\
               }\n\
               m[\"k\"] = 99\n\
               <-done\n\
             }",
        );
        assert_eq!(r_inner_peer_shadow_move.verdict, Verdict::Reject);
        assert!(r_inner_peer_shadow_move
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 5. Shadowing or rebinding a handle/composite binding while an A2 borrow
        // loan is live preserves the loan on surviving peers -> HYB-E3101.
        let r_a2_shadow = check(
            "import \"fmt\"\n\
             import \"rust/serde_json\"\n\
             struct holder { d serde_json.value }\n\
             fn bad() (int, error) {\n\
               doc := serde_json.parse(\"[]\")?\n\
               h := holder{d: doc}\n\
               u := serde_json.array(doc, \"users\")?\n\
               if true {\n\
                 doc := 1\n\
                 fmt.println(doc)\n\
                 _ = serde_json.set(h.d, \"pruned\", true)?\n\
               }\n\
               return u.len(), nil\n\
             }\n\
             fn main() {\n\
               _, _ = bad()\n\
             }",
        );
        assert_eq!(r_a2_shadow.verdict, Verdict::Reject);
        assert!(r_a2_shadow
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3101"));

        let r_a2_rebind = check(
            "import \"rust/serde_json\"\n\
             struct holder { d serde_json.value }\n\
             fn bad() (int, error) {\n\
               doc := serde_json.parse(\"[]\")?\n\
               h := holder{d: doc}\n\
               u := serde_json.array(doc, \"users\")?\n\
               doc = serde_json.parse(\"[]\")?\n\
               _ = serde_json.set(h.d, \"pruned\", true)?\n\
               return u.len(), nil\n\
             }\n\
             fn main() {\n\
               _, _ = bad()\n\
             }",
        );
        assert_eq!(r_a2_rebind.verdict, Verdict::Reject);
        assert!(r_a2_rebind
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3101"));

        // 6. Shadowing or rebinding a map/composite binding while an A3 slot loan
        // is live preserves the slot loan on surviving peers -> HYB-E3102.
        let r_a3_shadow = check(
            "import \"fmt\"\n\
             struct svc { n int }\n\
             struct holder { m map[string]svc }\n\
             fn main() {\n\
               m := map[string]svc{\"k\": svc{n: 1}}\n\
               h := holder{m: m}\n\
               slot := &m[\"k\"]\n\
               if true {\n\
                 m := 1\n\
                 fmt.println(m)\n\
                 delete(h.m, \"k\")\n\
               }\n\
               slot.n = 2\n\
               fmt.println(slot.n)\n\
             }",
        );
        assert_eq!(r_a3_shadow.verdict, Verdict::Reject);
        assert!(r_a3_shadow
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3102"));

        let r_a3_rebind = check(
            "import \"fmt\"\n\
             struct svc { n int }\n\
             struct holder { m map[string]svc }\n\
             fn main() {\n\
               m := map[string]svc{\"k\": svc{n: 1}}\n\
               h := holder{m: m}\n\
               slot := &h.m[\"k\"]\n\
               h = holder{m: map[string]svc{\"k\": svc{n: 2}}}\n\
               delete(m, \"k\")\n\
               slot.n = 5\n\
               fmt.println(slot.n, h.m[\"k\"].n)\n\
             }",
        );
        assert_eq!(r_a3_rebind.verdict, Verdict::Reject);
        assert!(r_a3_rebind
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3102"));

        // 7. `move x` inside an expression statement (`copy(dst, move src)`)
        // freezes `src` -> HYB-E3003 on subsequent use.
        let r_expr_stmt_move = check(
            "import \"fmt\"\n\
             fn main() {\n\
               src := []int{1, 2, 3}\n\
               dst := []int{0, 0, 0}\n\
               copy(dst, move src)\n\
               src[0] = 99\n\
               fmt.println(dst[0], src[0])\n\
             }",
        );
        assert_eq!(r_expr_stmt_move.verdict, Verdict::Reject);
        assert!(r_expr_stmt_move
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 8. Positive control: rebinding an inner peer `m2` to a fresh map BEFORE
        // moving `m2` while outer `m` is shadowed does NOT freeze outer `m`.
        let r_rebind_before_move_ok = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               if true {\n\
                 m2 := m\n\
                 _ = m2[\"k\"]\n\
                 m := map[string]int{\"fresh\": 2}\n\
                 _ = m\n\
                 m2 = map[string]int{\"fresh2\": 3}\n\
                 task worker(move m2, done)\n\
               }\n\
               m[\"k\"] = 99\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_rebind_before_move_ok.verdict,
            Verdict::Accept,
            "{:?}",
            r_rebind_before_move_ok.diagnostics
        );

        // 9. Multi-value passthrough rebind (`m, ok = pass(m)`) preserves `m`'s
        // may-share edge to `h` -> HYB-E3003.
        let r_multival_passthrough = check(
            "struct holder { m map[string]int }\n\
             fn pass(m map[string]int) (map[string]int, bool) { return m, true }\n\
             fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               h := holder{m: m}\n\
               ok := false\n\
               m, ok = pass(m)\n\
               _ = ok\n\
               task worker(move m, done)\n\
               h.m[\"k\"] = 99\n\
               <-done\n\
             }",
        );
        assert_eq!(r_multival_passthrough.verdict, Verdict::Reject);
        assert!(r_multival_passthrough
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"));

        // 10. Positive controls for child-alias detachment on shadow and rebind:
        // moving the fresh inner `m` (when `m2 := m` preceded the shadow) does NOT
        // freeze `m2` or outer `m`; and rebinding `a = fresh` (when `b := a`
        // preceded the rebind) followed by `move b` does NOT freeze the fresh `a`.
        let r_move_fresh_shadow_ok = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               done := make(chan bool, 1)\n\
               if true {\n\
                 m2 := m\n\
                 _ = m2[\"k\"]\n\
                 m := map[string]int{\"fresh\": 2}\n\
                 task worker(move m, done)\n\
               }\n\
               m[\"k\"] = 99\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_move_fresh_shadow_ok.verdict,
            Verdict::Accept,
            "{:?}",
            r_move_fresh_shadow_ok.diagnostics
        );

        let r_rebind_parent_then_move_child_ok = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               m[\"k\"] = 1\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               a := map[string]int{\"k\": 0}\n\
               b := a\n\
               a = map[string]int{\"fresh\": 1}\n\
               done := make(chan bool, 1)\n\
               task worker(move b, done)\n\
               a[\"fresh\"] = 2\n\
               <-done\n\
             }",
        );
        assert_eq!(
            r_rebind_parent_then_move_child_ok.verdict,
            Verdict::Accept,
            "{:?}",
            r_rebind_parent_then_move_child_ok.diagnostics
        );

        // 11. `move x` inside `defer`, `if`, and `for` conditions freezes `x` -> HYB-E3003.
        let r_defer_if_for_moves = check(
            "fn consume(m map[string]int) bool { return m[\"k\"] == 1 }\n\
             fn main() {\n\
               m1 := map[string]int{\"k\": 1}\n\
               defer consume(move m1)\n\
               m1[\"k\"] = 2\n\
               m2 := map[string]int{\"k\": 1}\n\
               if consume(move m2) {\n\
                 m2[\"k\"] = 3\n\
               }\n\
             }",
        );
        assert_eq!(r_defer_if_for_moves.verdict, Verdict::Reject);
        assert!(
            r_defer_if_for_moves
                .diagnostics
                .iter()
                .filter(|d| d.code == "HYB-E3003")
                .count()
                >= 2
        );
    }

    #[test]
    fn closure_managed_bare_and_missing_category_reject_e0003() {
        let bare = check(
            "fn main() {\n\
               ch := make(chan map[string]int, 1)\n\
               m := map[string]int{\"k\": 1}\n\
               f := fn() {\n\
                 managed {\n\
                   ch <- m\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(bare.verdict, Verdict::Reject, "{:?}", bare.diagnostics);
        let e0003_count = bare
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E0003")
            .count();
        assert_eq!(e0003_count, 2, "{:?}", bare.diagnostics);

        let no_cat = check(
            "fn main() {\n\
               ch := make(chan map[string]int, 1)\n\
               m := map[string]int{\"k\": 1}\n\
               f := fn() {\n\
                 managed(\"mutex guards map\") {\n\
                   ch <- m\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(no_cat.verdict, Verdict::Reject, "{:?}", no_cat.diagnostics);
        assert!(
            no_cat.diagnostics.iter().any(|d| d.code == "HYB-E0003"),
            "{:?}",
            no_cat.diagnostics
        );
    }

    #[test]
    fn closure_managed_unknown_category_rejects_e0004_and_valid_records_audit() {
        let bad = check(
            "fn main() {\n\
               ch := make(chan map[string]int, 1)\n\
               m := map[string]int{\"k\": 1}\n\
               f := fn() {\n\
                 managed(\"mutex guards map\", \"not-a-category\") {\n\
                   ch <- m\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(bad.verdict, Verdict::Reject, "{:?}", bad.diagnostics);
        assert!(
            bad.diagnostics.iter().any(|d| d.code == "HYB-E0004"),
            "{:?}",
            bad.diagnostics
        );

        let ok = check(
            "fn main() {\n\
               ch := make(chan map[string]int, 1)\n\
               m := map[string]int{\"k\": 1}\n\
               f := fn() {\n\
                 managed(\"mutex guards map\", \"mutex\", \"PROJ-9\") {\n\
                   ch <- m\n\
                 }\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
        assert_eq!(ok.managed.managed_blocks, 1);
        assert_eq!(ok.managed.blocks.len(), 1);
        assert_eq!(ok.managed.blocks[0].category, "mutex");
        assert_eq!(ok.managed.blocks[0].tickets, vec!["PROJ-9".to_string()]);
        assert_eq!(ok.managed.categories.get("mutex"), Some(&1));
    }

    #[test]
    fn closure_loan_escape_return_addr_of_and_literal_rejects_e3104() {
        let direct = check(
            "fn main() {\n\
               f := fn() *int {\n\
                 x := 1\n\
                 return &x\n\
               }\n\
               _ = f\n\
             }",
        );
        assert_eq!(direct.verdict, Verdict::Reject, "{:?}", direct.diagnostics);
        assert_eq!(direct.diagnostics[0].code, "HYB-E3104");

        let lit = check(
            "fn main() {\n\
               f := fn() []*int {\n\
                 x := 1\n\
                 return []*int{&x}\n\
               }\n\
               _ = f\n\
             }",
        );
        assert_eq!(lit.verdict, Verdict::Reject, "{:?}", lit.diagnostics);
        assert_eq!(lit.diagnostics[0].code, "HYB-E3104");
    }

    #[test]
    fn closure_a6_field_borrow_store_rejects_e3103() {
        let addr = check(
            "struct holder { p int }\n\
             fn main() {\n\
               h := holder{p: 0}\n\
               f := fn() {\n\
                 x := 1\n\
                 h.p = &x\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(addr.verdict, Verdict::Reject, "{:?}", addr.diagnostics);
        assert!(
            addr.diagnostics.iter().any(|d| d.code == "HYB-E3103"),
            "{:?}",
            addr.diagnostics
        );

        let view = check(
            "import \"serde_json\"\n\
             struct holder { v serde_json.value }\n\
             fn main() {\n\
               doc := serde_json.parse(\"[1, 2]\")\n\
               h := holder{v: serde_json.parse(\"[]\")}\n\
               f := fn() {\n\
                 h.v = doc.array()\n\
               }\n\
               f()\n\
             }",
        );
        assert_eq!(view.verdict, Verdict::Reject, "{:?}", view.diagnostics);
        assert!(
            view.diagnostics.iter().any(|d| d.code == "HYB-E3103"),
            "{:?}",
            view.diagnostics
        );
    }

    #[test]
    fn closure_n1_local_assign_return_and_composite_nil_reject_e4001() {
        let r = check(
            "struct job { m map[string]int }\n\
             fn main() {\n\
               f := fn() []int {\n\
                 var s []int = nil\n\
                 s = nil\n\
                 j := job{m: nil}\n\
                 _ = j\n\
                 return nil\n\
               }\n\
               _ = f\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let e4001_count = r
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E4001")
            .count();
        assert_eq!(e4001_count, 4, "{:?}", r.diagnostics);
    }

    #[test]
    fn closure_n1_nilable_option_result_and_value_return_accept() {
        let r = check(
            "fn main() {\n\
               f := fn() (int, error) {\n\
                 return 0, nil\n\
               }\n\
               g := fn() int {\n\
                 x := 7\n\
                 return x\n\
               }\n\
               _ = f\n\
               _ = g\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn move_of_const_and_const_alias_preserve_t22_e3006() {
        let direct = check(
            "const k_slice = []int{1, 2}\n\
             fn main() {\n\
               c := move k_slice\n\
               c[0] = 99\n\
             }",
        );
        assert_eq!(direct.verdict, Verdict::Reject, "{:?}", direct.diagnostics);
        assert!(
            direct.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
            "expected HYB-E3006 after move-define of const, got {:?}",
            direct.diagnostics
        );

        let rebind_and_delete = check(
            "const k_map = map[string]int{\"a\": 1}\n\
             fn main() {\n\
               alias := k_map\n\
               var c map[string]int\n\
               c = move alias\n\
               delete(c, \"a\")\n\
             }",
        );
        assert_eq!(
            rebind_and_delete.verdict,
            Verdict::Reject,
            "{:?}",
            rebind_and_delete.diagnostics
        );
        assert!(
            rebind_and_delete
                .diagnostics
                .iter()
                .any(|d| d.code == "HYB-E3006"),
            "expected HYB-E3006 for delete through move-rebound const alias, got {:?}",
            rebind_and_delete.diagnostics
        );

        let lit = check(
            "struct holder { s []int }\n\
             const k_slice = []int{1, 2}\n\
             fn main() {\n\
               h := holder{s: move k_slice}\n\
               h.s[0] = 99\n\
             }",
        );
        assert_eq!(lit.verdict, Verdict::Reject, "{:?}", lit.diagnostics);
        assert!(
            lit.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
            "expected HYB-E3006 for write through struct literal move of const, got {:?}",
            lit.diagnostics
        );
    }

    #[test]
    fn move_of_map_during_live_slot_loan_preserves_a3_e3102() {
        let define = check(
            "import \"fmt\"\n\
             struct item { n int }\n\
             fn main() {\n\
               m := map[string]item{\"a\": item{n: 1}}\n\
               slot := &m[\"a\"]\n\
               m2 := move m\n\
               m2[\"b\"] = item{n: 2}\n\
               fmt.println(slot.n)\n\
             }",
        );
        assert_eq!(define.verdict, Verdict::Reject, "{:?}", define.diagnostics);
        assert!(
            define.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
            "expected HYB-E3102 on insert through moved map during live slot loan, got {:?}",
            define.diagnostics
        );

        let rebind_delete = check(
            "import \"fmt\"\n\
             struct item { n int }\n\
             fn main() {\n\
               m := map[string]item{\"a\": item{n: 1}}\n\
               slot := &m[\"a\"]\n\
               var m2 map[string]item\n\
               m2 = move m\n\
               delete(m2, \"a\")\n\
               fmt.println(slot.n)\n\
             }",
        );
        assert_eq!(
            rebind_delete.verdict,
            Verdict::Reject,
            "{:?}",
            rebind_delete.diagnostics
        );
        assert!(
            rebind_delete
                .diagnostics
                .iter()
                .any(|d| d.code == "HYB-E3102"),
            "expected HYB-E3102 on delete through move-rebound map during live slot loan, got {:?}",
            rebind_delete.diagnostics
        );
    }

    #[test]
    fn move_of_map_after_slot_loan_last_use_accepts() {
        let r = check(
            "import \"fmt\"\n\
             struct item { n int }\n\
             fn main() {\n\
               m := map[string]item{\"a\": item{n: 1}}\n\
               slot := &m[\"a\"]\n\
               fmt.println(slot.n)\n\
               m2 := move m\n\
               m2[\"b\"] = item{n: 2}\n\
               fmt.println(m2[\"b\"].n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn wrapped_move_of_local_freezes_source_on_field_index_slice_and_call() {
        // 1. `t := box{m: move m}.m` then boundary-cross then original write -> HYB-E3003
        let field_proj = check(
            "struct box { m map[string]int }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               t := box{m: move m}.m\n\
               task worker(move t)\n\
               m[\"main\"] = 2\n\
             }",
        );
        assert_eq!(field_proj.verdict, Verdict::Reject, "{:?}", field_proj.diagnostics);
        assert_eq!(field_proj.diagnostics[0].code, "HYB-E3003");
        assert_eq!(first_error_rule(&field_proj).as_deref(), Some("A1b"));

        // 2. `t := []map[string]int{move m}[0]` then boundary-cross then original write -> HYB-E3003
        let index_proj = check(
            "fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               t := []map[string]int{move m}[0]\n\
               task worker(move t)\n\
               m[\"main\"] = 2\n\
             }",
        );
        assert_eq!(index_proj.verdict, Verdict::Reject, "{:?}", index_proj.diagnostics);
        assert_eq!(index_proj.diagnostics[0].code, "HYB-E3003");
        assert_eq!(first_error_rule(&index_proj).as_deref(), Some("A1b"));

        // 3. `t := (move s)[0:2]` then boundary-cross then original write -> HYB-E3003
        let slice_proj = check(
            "fn worker(x []int) { x[0] = 1 }\n\
             fn main() {\n\
               s := []int{1, 2, 3}\n\
               t := (move s)[0:2]\n\
               task worker(move t)\n\
               s[0] = 9\n\
             }",
        );
        assert_eq!(slice_proj.verdict, Verdict::Reject, "{:?}", slice_proj.diagnostics);
        assert_eq!(slice_proj.diagnostics[0].code, "HYB-E3003");
        assert_eq!(first_error_rule(&slice_proj).as_deref(), Some("A1b"));

        // 4. `t := same(move m)` then boundary-cross then original write -> HYB-E3003
        let call_wrap = check(
            "fn same(x map[string]int) map[string]int { return x }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               t := same(move m)\n\
               task worker(move t)\n\
               m[\"main\"] = 2\n\
             }",
        );
        assert_eq!(call_wrap.verdict, Verdict::Reject, "{:?}", call_wrap.diagnostics);
        assert_eq!(call_wrap.diagnostics[0].code, "HYB-E3003");
        assert_eq!(first_error_rule(&call_wrap).as_deref(), Some("A1b"));

        // Positive control: wrapped move with no post-move use of `m` accepts.
        let clean = check(
            "struct box { m map[string]int }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"init\": 0}\n\
               t := box{m: move m}.m\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(clean.verdict, Verdict::Accept, "{:?}", clean.diagnostics);
    }

    #[test]
    fn wrapped_move_of_non_owned_param_preserves_origin_e3004() {
        // 1. `t := box{m: move m}.m` on parameter `m` -> HYB-E3004
        let field_proj = check(
            "struct box { m map[string]int }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn bad(m map[string]int) {\n\
               t := box{m: move m}.m\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(field_proj.verdict, Verdict::Reject, "{:?}", field_proj.diagnostics);
        assert!(
            field_proj.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            field_proj.diagnostics
        );

        // 2. `t := []map[string]int{move m}[0]` on parameter `m` -> HYB-E3004
        let index_proj = check(
            "fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn bad(m map[string]int) {\n\
               t := []map[string]int{move m}[0]\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(index_proj.verdict, Verdict::Reject, "{:?}", index_proj.diagnostics);
        assert!(
            index_proj.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            index_proj.diagnostics
        );

        // 3. `t := (move s)[0:2]` on parameter `s` -> HYB-E3004
        let slice_proj = check(
            "fn worker(x []int) { x[0] = 1 }\n\
             fn bad(s []int) {\n\
               t := (move s)[0:2]\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(slice_proj.verdict, Verdict::Reject, "{:?}", slice_proj.diagnostics);
        assert!(
            slice_proj.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            slice_proj.diagnostics
        );

        // 4. `t := same(move m)` on parameter `m` -> HYB-E3004
        let call_wrap = check(
            "fn same(x map[string]int) map[string]int { return x }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn bad(m map[string]int) {\n\
               t := same(move m)\n\
               task worker(move t)\n\
             }",
        );
        assert_eq!(call_wrap.verdict, Verdict::Reject, "{:?}", call_wrap.diagnostics);
        assert!(
            call_wrap.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            call_wrap.diagnostics
        );

        // 5. Rebind `t = box{m: move m}.m` and self-rebind `m = same(move m)` on parameter `m` -> HYB-E3004
        let rebind_wrap = check(
            "struct box { m map[string]int }\n\
             fn same(x map[string]int) map[string]int { return x }\n\
             fn worker(x map[string]int) { x[\"w\"] = 1 }\n\
             fn bad(m map[string]int) {\n\
               var t map[string]int\n\
               t = box{m: move m}.m\n\
               task worker(move t)\n\
             }\n\
             fn bad_self(m map[string]int) {\n\
               m = same(move m)\n\
               task worker(move m)\n\
             }",
        );
        assert_eq!(rebind_wrap.verdict, Verdict::Reject, "{:?}", rebind_wrap.diagnostics);
        let e3004_count = rebind_wrap
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3004")
            .count();
        assert_eq!(e3004_count, 2, "{:?}", rebind_wrap.diagnostics);
    }

    #[test]
    fn wrapped_move_of_const_composite_in_call_and_delete_rejects_e3006() {
        let r = check(
            "struct box { m map[string]int }\n\
             const shared_m = map[string]int{\"k\": 1}\n\
             fn mut_via_move(p map[string]int) {\n\
               t := move p\n\
               t[\"k\"] = 2\n\
             }\n\
             fn main() {\n\
               delete(move shared_m, \"k\")\n\
               mut_via_move(box{m: move shared_m}.m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let e3006_count = r
            .diagnostics
            .iter()
            .filter(|d| d.code == "HYB-E3006")
            .count();
        assert_eq!(e3006_count, 2, "{:?}", r.diagnostics);
    }
}

