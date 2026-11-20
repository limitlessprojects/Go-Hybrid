//! managed_contract.rs — Verifiable synchronization contracts for `managed` blocks (#4297).
//!
//! Turns `managed("reason", "category", ...)` from a purely syntactic audit
//! marker into a fail-closed proof obligation for synchronization categories:
//!   * `"mutex"`: requires `guard: <mutex_path>, protects: <targets>`. Every
//!     read/write to a protected root or field inside the block, inside any
//!     spawned `task` or helper `fn` receiving the shared state, and after the
//!     block (for outer-scoped roots that spawned tasks may outlive) must occur
//!     within a balanced `<guard>.lock()` ... `<guard>.unlock()` critical
//!     section (`rlock()`/`runlock()` permits reads only on `sync.rw_mutex`).
//!   * `"atomic"`: requires `protects: <targets>` (no `guard:`). Every access
//!     to a protected target must go through `atomic.*` calls or atomic methods;
//!     raw reads, writes, increments, and aliases are rejected (`HYB-E0005`).
//!   * `"interop"` / `"finalizer"`: remain explicit unverified perimeter
//!     hatches (`verified: false`, counted under `unverified_blocks`).

use crate::foreign;
use crate::model::{as_addr_of, unwrap_parens, unwrap_parens_and_move, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, ManagedContractAudit, Span};
use hy_syntax::{
    AssignOp, Block, CallArg, Decl, Expr, FnDecl, ForKind, InterpPart, LocalDecl, ManagedContract,
    Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
enum ContractMode {
    Mutex { guard_path: String, is_rw: bool },
    Atomic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LockState {
    Unlocked,
    ReadLocked,
    WriteLocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathMatch {
    None,
    Guard,
    Protected,
    UncoveredSiblingField,
}

struct VerifyCtx {
    mode: ContractMode,
    protects: Vec<String>,
    shared_roots: HashSet<String>,
    local_shapes: HashMap<String, TypeShape>,
    in_root_managed_block: bool,
    saw_concurrency_boundary: bool,
    deferred_unlock: bool,
    in_loop: bool,
}

impl VerifyCtx {
    fn sub(mode: ContractMode, protects: Vec<String>, shared_roots: HashSet<String>, local_shapes: HashMap<String, TypeShape>, deferred_unlock: bool) -> Self {
        Self { mode, protects, shared_roots, local_shapes, in_root_managed_block: false, saw_concurrency_boundary: true, deferred_unlock, in_loop: false }
    }
}

impl<'a> Checker<'a> {
    /// Validate and mechanically check a `managed` block's synchronization contract (#4297).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn check_managed_contract(&mut self, st: &FnState, reason: &str, category: &str, contract: Option<&ManagedContract>, body: &Block, enclosing_fn_body: Option<&Block>, span: Span) -> Option<ManagedContractAudit> {
        let valid_cat = crate::model::MANAGED_CATEGORIES.contains(&category);
        let Some(c) = contract else {
            if self.require_managed_contracts && valid_cat && !reason.trim().is_empty() && !matches!(category, "interop" | "finalizer") {
                let req = if category == "mutex" { "`guard:` and `protects:`" } else { "`protects:`" };
                let fix = match category {
                    "mutex" => "fix: add `guard: <mutex_path>, protects: <shared_root>` to the `managed(...)` header".to_string(),
                    "atomic" => "fix: add `protects: <atomic_root>` to the `managed(...)` header".to_string(),
                    _ => format!("fix: migrate this `{category}` region to a checked `\"mutex\"` or `\"atomic\"` contract, or use channels / `move`"),
                };
                self.emit_contract_err(span, format!("`managed(\"...\", \"{category}\")` has no synchronization contract — verifiable categories must declare {req} under `--require-managed-contracts` (HYB-E0005)"), vec![
                    "why: auditability of presence is not evidence of synchronization (#4297); only `\"interop\"` and `\"finalizer\"` may remain uncontracted perimeter hatches".to_string(),
                    fix,
                ]);
            }
            return None;
        };

        let diags_before = self.diags.len();
        for (msg, inv_span) in &c.invalid {
            self.err_note(*inv_span, format!("invalid managed synchronization contract: {msg} (HYB-E0005)"), "valid contract clauses are `guard: <mutex_path>` (for `\"mutex\"`) and `protects: <path>` or `protects: [p1, p2]`");
        }

        let audit_guard = c.guard.as_ref().map(|(g, _)| g.clone());
        let audit_protects: Vec<String> = c.protects.iter().map(|(p, _)| p.clone()).collect();
        let unverified = |g, p| Some(ManagedContractAudit { guard: g, protects: p, verified: false });

        if !valid_cat || reason.trim().is_empty() {
            return unverified(audit_guard, audit_protects);
        }
        if !matches!(category, "mutex" | "atomic") {
            self.err_note(c.span, format!("`managed` category `\"{category}\"` does not support a machine-checked contract in Tier 0 — only `\"mutex\"` and `\"atomic\"` are verifiable disciplines (HYB-E0005)"), "`\"interop\"` and `\"finalizer\"` remain explicit unverified hatches; remove `guard:`/`protects:` or switch to `\"mutex\"`/`\"atomic\"`");
            return unverified(audit_guard, audit_protects);
        }

        let outer_shapes = self.collect_scope_shapes(st);
        let mut block_shapes = outer_shapes.clone();
        for s in &body.stmts {
            if let Stmt::Local(d) = s { self.record_local_decl_shapes(st, d, &mut block_shapes); }
        }

        let mode = if category == "mutex" {
            let Some((guard_path, guard_span)) = &c.guard else {
                self.err_note(c.span, "`managed` `\"mutex\"` contract requires `guard: <mutex_path>` naming a `sync.mutex` or `sync.rw_mutex` (HYB-E0005)", "example: `managed(\"pool stats\", \"mutex\", guard: stats.mu, protects: stats) { ... }`");
                return unverified(audit_guard, audit_protects);
            };
            let guard_shape = self.resolve_contract_path(&block_shapes, guard_path);
            let (is_mu, is_rw) = guard_shape.as_ref().map(is_mutex_shape).unwrap_or((false, false));
            if !is_mu {
                let found = guard_shape.as_ref().map(|s| s.render()).unwrap_or_else(|| "unresolved path".to_string());
                self.err_note(*guard_span, format!("managed contract guard `{guard_path}` resolved to `{found}`, expected `sync.mutex` or `sync.rw_mutex` (HYB-E0005)"), "import `\"sync\"` and point `guard:` at a `sync.mutex` or `sync.rw_mutex` binding or struct field");
            }
            if c.protects.is_empty() {
                self.err_note(c.span, "`managed` `\"mutex\"` contract requires `protects: <target>` naming the shared state guarded by the lock (HYB-E0005)", "add `protects: <binding_or_field>` alongside `guard:`");
            }
            ContractMode::Mutex { guard_path: guard_path.clone(), is_rw }
        } else {
            if let Some((_, gspan)) = &c.guard {
                self.err_note(*gspan, "`managed` `\"atomic\"` contract cannot specify `guard:` — atomic contracts verify `atomic.*` operations on `protects:` targets directly (HYB-E0005)", "remove `guard:` and keep `protects: <atomic_target>`");
            }
            if c.protects.is_empty() {
                self.err_note(c.span, "`managed` `\"atomic\"` contract requires `protects: <target>` naming the atomic state (HYB-E0005)", "example: `managed(\"shared counter\", \"atomic\", protects: counter) { ... }`");
            }
            ContractMode::Atomic
        };

        let mut protects = Vec::new();
        let mut shared_roots = HashSet::new();
        for (p, pspan) in &c.protects {
            shared_roots.insert(path_root(p).to_string());
            if let ContractMode::Mutex { guard_path, .. } = &mode {
                if p == guard_path {
                    self.err_note(*pspan, format!("`protects: {p}` names the mutex guard itself — `protects:` must name the data guarded by `{guard_path}` (HYB-E0005)"), "point `protects:` at the shared struct, field, map, or binding protected by the lock");
                }
            }
            match self.resolve_contract_path_ext(st, &body.stmts, &block_shapes, p) {
                None => self.err_note(*pspan, format!("managed contract target `{p}` does not resolve to a binding or struct field in scope (HYB-E0005)"), "declare the protected binding before or at the top of the `managed` block, and check field names"),
                Some((pshape, pty_opt)) if matches!(mode, ContractMode::Atomic) && !is_valid_atomic_target(&pshape, pty_opt.as_ref()) => {
                    self.err_note(*pspan, format!("managed `\"atomic\"` contract target `{p}` has non-atomic type `{}` — composite GC containers cannot be synchronized via `atomic.*` (HYB-E0005)", pshape.render()), "use an integer/pointer/`atomic.*` binding with `atomic.*` calls, or switch to a `\"mutex\"` contract with a `sync.mutex` guard");
                }
                Some(_) => {}
            }
            protects.push(p.clone());
        }
        if self.diags.len() > diags_before {
            return unverified(audit_guard, audit_protects);
        }

        let mut ctx = VerifyCtx { mode: mode.clone(), protects: protects.clone(), shared_roots, local_shapes: outer_shapes.clone(), in_root_managed_block: true, saw_concurrency_boundary: false, deferred_unlock: false, in_loop: false };
        let mut visited_fns = HashSet::new();
        let (end_state, diverges) = self.verify_contract_stmts(st, &mut ctx, &body.stmts, LockState::Unlocked, &mut visited_fns);
        self.check_exit_unlocked(&ctx, end_state, diverges, body.span, "the `managed` block");

        let outer_protects: Vec<String> = protects.into_iter().filter(|p| outer_shapes.contains_key(path_root(p))).collect();
        if ctx.saw_concurrency_boundary && !outer_protects.is_empty() {
            if let Some(fn_body) = enclosing_fn_body {
                let post_stmts: Vec<Stmt> = fn_body.stmts.iter().filter(|s| (s.span().start.line, s.span().start.col) > (span.end.line, span.end.col)).cloned().collect();
                if !post_stmts.is_empty() {
                    let roots = outer_protects.iter().map(|p| path_root(p).to_string()).collect();
                    let mut post_ctx = VerifyCtx::sub(mode, outer_protects, roots, outer_shapes, ctx.deferred_unlock);
                    let (p_end, p_div) = self.verify_contract_stmts(st, &mut post_ctx, &post_stmts, LockState::Unlocked, &mut visited_fns);
                    self.check_exit_unlocked(&post_ctx, p_end, p_div, fn_body.span, "function exit");
                }
            }
        }
        Some(ManagedContractAudit { guard: audit_guard, protects: audit_protects, verified: self.diags.len() == diags_before })
    }

    fn verify_contract_stmts(
        &mut self,
        st: &FnState,
        ctx: &mut VerifyCtx,
        stmts: &[Stmt],
        mut state: LockState,
        visited_fns: &mut HashSet<String>,
    ) -> (LockState, bool) {
        let saved_shapes = ctx.local_shapes.clone();
        let mut diverges = false;

        for stmt in stmts {
            if diverges {
                break;
            }
            match stmt {
                Stmt::Local(d) => {
                    let is_prot_init = ctx.in_root_managed_block
                        && !ctx.saw_concurrency_boundary
                        && d.names.iter().any(|n| ctx.shared_roots.contains(n));
                    for val in &d.values {
                        if !is_prot_init {
                            self.check_no_protected_alias(st, ctx, val, d.span);
                        }
                        self.verify_contract_expr(st, ctx, val, state, visited_fns);
                    }
                    self.record_local_decl_shapes(st, d, &mut ctx.local_shapes);
                }
                Stmt::Assign { targets, op, values, span } => {
                    for val in values {
                        self.check_no_protected_alias(st, ctx, val, *span);
                        self.verify_contract_expr(st, ctx, val, state, visited_fns);
                    }
                    for t in targets {
                        self.verify_place_write(st, ctx, t, *op, state, visited_fns, *span);
                    }
                }
                Stmt::Expr { expr, span } => {
                    if let Some(next_state) = self.try_mutex_transition(ctx, expr, state, *span) {
                        state = next_state;
                    } else {
                        self.verify_contract_expr(st, ctx, expr, state, visited_fns);
                    }
                }
                Stmt::Defer { call, span } => {
                    if self.try_defer_unlock(ctx, call, state, *span) {
                        ctx.deferred_unlock = true;
                    } else {
                        self.verify_contract_expr(st, ctx, call, state, visited_fns);
                    }
                }
                Stmt::Task { callee, args, span } => {
                    ctx.saw_concurrency_boundary = true;
                    self.verify_task_spawn(st, ctx, callee, args, state, *span, visited_fns);
                }
                Stmt::Send { chan, value, is_move, span } => {
                    ctx.saw_concurrency_boundary = true;
                    self.verify_contract_expr(st, ctx, chan, state, visited_fns);
                    self.verify_channel_send(st, ctx, value, *is_move, state, *span, visited_fns);
                }
                Stmt::If { init, cond, then, otherwise, span } => {
                    let pre_if = ctx.local_shapes.clone();
                    if let Some(i) = init {
                        (state, _) = self.verify_contract_stmts(st, ctx, std::slice::from_ref(i.as_ref()), state, visited_fns);
                    }
                    self.verify_contract_expr(st, ctx, cond, state, visited_fns);
                    let (then_s, then_d) = self.verify_contract_stmts(st, ctx, &then.stmts, state, visited_fns);
                    let (else_s, else_d) = match otherwise {
                        Some(o) => self.verify_contract_stmts(st, ctx, std::slice::from_ref(o.as_ref()), state, visited_fns),
                        None => (state, false),
                    };
                    ctx.local_shapes = pre_if;
                    match (then_d, else_d) {
                        (true, true) => diverges = true,
                        (false, true) => state = then_s,
                        (true, false) => state = else_s,
                        (false, false) if then_s == else_s => state = then_s,
                        (false, false) => {
                            if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                                self.err_note(
                                    *span,
                                    format!("inconsistent lock state for `{guard_path}` across `if` branches — every branch must leave the mutex in the same state (HYB-E0005)"),
                                    format!("ensure both branches either keep `{guard_path}` unlocked or unlock `{guard_path}` before joining"),
                                );
                            }
                            state = LockState::Unlocked;
                        }
                    }
                }
                Stmt::For { kind, body, span } => {
                    let pre_for = ctx.local_shapes.clone();
                    match kind {
                        ForKind::Infinite => {}
                        ForKind::While { cond } => self.verify_contract_expr(st, ctx, cond, state, visited_fns),
                        ForKind::Range { var, lo, hi, .. } => {
                            self.verify_contract_expr(st, ctx, lo, state, visited_fns);
                            self.verify_contract_expr(st, ctx, hi, state, visited_fns);
                            if var != "_" {
                                ctx.local_shapes.insert(var.clone(), TypeShape::Primitive);
                            }
                        }
                        ForKind::Iterate { vars, expr } => {
                            self.verify_contract_expr(st, ctx, expr, state, visited_fns);
                            let iter_sh = self.expr_shape_in_ctx(st, ctx, expr);
                            for (i, v) in vars.iter().enumerate() {
                                if v != "_" {
                                    ctx.local_shapes.insert(v.clone(), iterate_var_shape(&iter_sh, i, vars.len()));
                                }
                            }
                        }
                    }
                    let prev_loop = ctx.in_loop;
                    ctx.in_loop = true;
                    let (body_s, body_d) = self.verify_contract_stmts(st, ctx, &body.stmts, state, visited_fns);
                    ctx.in_loop = prev_loop;
                    ctx.local_shapes = pre_for;
                    if !body_d && body_s != state {
                        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                            self.err_note(
                                *span,
                                format!("loop body changes lock state of `{guard_path}` across iterations (HYB-E0005)"),
                                format!("balance `{guard_path}.lock()` and `{guard_path}.unlock()` within the same loop iteration"),
                            );
                        }
                        state = LockState::Unlocked;
                    }
                }
                Stmt::Match { subject, arms, span } => {
                    self.verify_contract_expr(st, ctx, subject, state, visited_fns);
                    let mut joined: Option<LockState> = None;
                    let mut all_div = !arms.is_empty();
                    for arm in arms {
                        if let Some(g) = &arm.guard {
                            self.verify_contract_expr(st, ctx, g, state, visited_fns);
                        }
                        let (arm_s, arm_d) = self.verify_contract_stmts(st, ctx, &arm.body.stmts, state, visited_fns);
                        if !arm_d {
                            all_div = false;
                            joined = Some(self.join_arm_state(ctx, joined, arm_s, *span, "match"));
                        }
                    }
                    if all_div { diverges = true; } else if let Some(j) = joined { state = j; }
                }
                Stmt::Select { arms, span } => {
                    let mut joined: Option<LockState> = None;
                    let mut all_div = !arms.is_empty();
                    for arm in arms {
                        let pre_arm = ctx.local_shapes.clone();
                        match &arm.kind {
                            SelectArmKind::Send { chan, value, is_move } => {
                                ctx.saw_concurrency_boundary = true;
                                self.verify_contract_expr(st, ctx, chan, state, visited_fns);
                                self.verify_channel_send(st, ctx, value, *is_move, state, *span, visited_fns);
                            }
                            SelectArmKind::Recv { names, chan } => {
                                self.verify_contract_expr(st, ctx, chan, state, visited_fns);
                                for n in names.iter().filter(|n| n.as_str() != "_") {
                                    ctx.local_shapes.insert(n.clone(), TypeShape::Unknown);
                                }
                            }
                            SelectArmKind::Default => {}
                        }
                        let (arm_s, arm_d) = self.verify_contract_stmts(st, ctx, &arm.body.stmts, state, visited_fns);
                        ctx.local_shapes = pre_arm;
                        if !arm_d {
                            all_div = false;
                            joined = Some(self.join_arm_state(ctx, joined, arm_s, *span, "select"));
                        }
                    }
                    if all_div { diverges = true; } else if let Some(j) = joined { state = j; }
                }
                Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
                    (state, diverges) = self.verify_contract_stmts(st, ctx, &block.stmts, state, visited_fns);
                }
                Stmt::Return { values, span } => {
                    for v in values {
                        self.check_no_protected_alias(st, ctx, v, *span);
                        self.verify_contract_expr(st, ctx, v, state, visited_fns);
                    }
                    if state != LockState::Unlocked && !ctx.deferred_unlock {
                        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                            self.err_note(
                                *span,
                                format!("`return` leaves mutex guard `{guard_path}` locked without `defer {guard_path}.unlock()` (HYB-E0005)"),
                                format!("call `{guard_path}.unlock()` before returning, or register `defer {guard_path}.unlock()`"),
                            );
                        }
                    }
                    diverges = true;
                }
                Stmt::Break { span, .. } | Stmt::Continue { span, .. } => {
                    if state != LockState::Unlocked {
                        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                            self.err_note(
                                *span,
                                format!("`break`/`continue` exits loop while mutex guard `{guard_path}` is still locked (HYB-E0005)"),
                                format!("call `{guard_path}.unlock()` before breaking or continuing"),
                            );
                        }
                    }
                    diverges = true;
                }
            }
        }

        ctx.local_shapes = saved_shapes;
        (state, diverges)
    }

    fn check_exit_unlocked(&mut self, ctx: &VerifyCtx, state: LockState, diverges: bool, span: Span, where_desc: &str) {
        if !diverges && state != LockState::Unlocked && !ctx.deferred_unlock {
            if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                self.err_note(
                    span,
                    format!("mutex guard `{guard_path}` remains locked at {where_desc} — missing `{guard_path}.unlock()` (HYB-E0005)"),
                    format!("balance `{guard_path}.lock()` with `{guard_path}.unlock()` before leaving {where_desc}"),
                );
            }
        }
    }

    fn join_arm_state(&mut self, ctx: &VerifyCtx, prev: Option<LockState>, arm_s: LockState, span: Span, kind: &str) -> LockState {
        let Some(p) = prev else { return arm_s; };
        if p == arm_s { return p; }
        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
            self.err(span, format!("inconsistent lock state for `{guard_path}` across `{kind}` arms (HYB-E0005)"));
        }
        LockState::Unlocked
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_place_write(
        &mut self,
        st: &FnState,
        ctx: &mut VerifyCtx,
        place: &Place,
        op: AssignOp,
        state: LockState,
        visited_fns: &mut HashSet<String>,
        span: Span,
    ) {
        self.verify_place_indices(st, ctx, place, state, visited_fns);
        let Some(path) = place_dotted_path(place) else { return; };
        match classify_path(&path, &ctx.mode, &ctx.protects, &ctx.shared_roots) {
            PathMatch::None => {}
            PathMatch::Guard => self.err_note(
                span,
                format!("cannot overwrite or mutate mutex guard `{path}` inside a `managed` contract (HYB-E0005)"),
                "only `.lock()` / `.unlock()` (or `.rlock()` / `.runlock()`) may operate on the declared guard",
            ),
            PathMatch::UncoveredSiblingField => self.err_note(
                span,
                format!("write to `{path}` touches shared root `{}` outside declared contract access set `protects: [{}]` (HYB-E0005)", path_root(&path), ctx.protects.join(", ")),
                format!("add `{path}` (or whole `{}`) to `protects:` and guard every access under the contract", path_root(&path)),
            ),
            PathMatch::Protected => match &ctx.mode {
                ContractMode::Atomic => {
                    let op_desc = if matches!(op, AssignOp::Inc | AssignOp::Dec) { "non-atomic increment/decrement" } else { "raw non-atomic write" };
                    self.err_note(
                        span,
                        format!("{op_desc} on `{path}` violates `\"atomic\"` contract — mutate exclusively via `atomic.*` operations (HYB-E0005)"),
                        format!("replace direct assignment/increment on `{path}` with `atomic.store_*` / `atomic.add_*` / `.store(...)` / `.add(...)`"),
                    );
                }
                ContractMode::Mutex { guard_path, .. } => match state {
                    LockState::WriteLocked => {}
                    LockState::ReadLocked => self.err_note(
                        span,
                        format!("write to `{path}` under `{guard_path}.rlock()` violates `\"mutex\"` contract — writes require exclusive `{guard_path}.lock()` (HYB-E0005)"),
                        format!("acquire `{guard_path}.lock()` instead of `{guard_path}.rlock()` before mutating `{path}`"),
                    ),
                    LockState::Unlocked => self.err_note(
                        span,
                        format!("unguarded write to `{path}` outside `{guard_path}.lock()` ... `{guard_path}.unlock()` critical section (HYB-E0005)"),
                        format!("acquire `{guard_path}.lock()` before writing `{path}` and call `{guard_path}.unlock()` afterward"),
                    ),
                },
            },
        }
    }

    fn verify_place_indices(&mut self, st: &FnState, ctx: &mut VerifyCtx, place: &Place, state: LockState, visited_fns: &mut HashSet<String>) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => self.verify_place_indices(st, ctx, base, state, visited_fns),
            PlaceKind::Index { base, index } => {
                self.verify_place_indices(st, ctx, base, state, visited_fns);
                self.verify_contract_expr(st, ctx, index, state, visited_fns);
            }
        }
    }

    fn verify_contract_expr(&mut self, st: &FnState, ctx: &mut VerifyCtx, expr: &Expr, state: LockState, visited_fns: &mut HashSet<String>) {
        let expr = unwrap_parens(expr);
        if let Expr::Call { callee, args, span } = expr {
            if self.verify_contract_call(st, ctx, callee, args, state, *span, visited_fns) {
                return;
            }
        }
        if let Some(path) = expr_dotted_path(expr) {
            self.verify_path_read(ctx, &path, state, expr.span());
            match expr {
                Expr::Index { index, .. } => self.verify_contract_expr(st, ctx, index, state, visited_fns),
                Expr::Slice { lo, hi, .. } => {
                    if let Some(l) = lo { self.verify_contract_expr(st, ctx, l, state, visited_fns); }
                    if let Some(h) = hi { self.verify_contract_expr(st, ctx, h, state, visited_fns); }
                }
                _ => {}
            }
            return;
        }
        match expr {
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Field { base, .. } => self.verify_contract_expr(st, ctx, base, state, visited_fns),
            Expr::Index { base, index, .. } => {
                self.verify_contract_expr(st, ctx, base, state, visited_fns);
                self.verify_contract_expr(st, ctx, index, state, visited_fns);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.verify_contract_expr(st, ctx, base, state, visited_fns);
                if let Some(l) = lo { self.verify_contract_expr(st, ctx, l, state, visited_fns); }
                if let Some(h) = hi { self.verify_contract_expr(st, ctx, h, state, visited_fns); }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.verify_contract_expr(st, ctx, lhs, state, visited_fns);
                self.verify_contract_expr(st, ctx, rhs, state, visited_fns);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.verify_contract_expr(st, ctx, operand, state, visited_fns),
            Expr::Receive { chan, .. } => self.verify_contract_expr(st, ctx, chan, state, visited_fns),
            Expr::Interp { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(e) = part { self.verify_contract_expr(st, ctx, e, state, visited_fns); }
                }
            }
            Expr::Call { callee, args, .. } => {
                self.verify_contract_expr(st, ctx, callee, state, visited_fns);
                for a in args { self.verify_contract_expr(st, ctx, &a.value, state, visited_fns); }
            }
            Expr::StructLit { fields, span, .. } => {
                for f in fields {
                    self.check_no_protected_alias(st, ctx, &f.value, *span);
                    self.verify_contract_expr(st, ctx, &f.value, state, visited_fns);
                }
            }
            Expr::SliceLit { items, span, .. } => {
                for item in items {
                    self.check_no_protected_alias(st, ctx, item, *span);
                    self.verify_contract_expr(st, ctx, item, state, visited_fns);
                }
            }
            Expr::MapLit { entries, span, .. } => {
                for (k, v) in entries {
                    self.verify_contract_expr(st, ctx, k, state, visited_fns);
                    self.check_no_protected_alias(st, ctx, v, *span);
                    self.verify_contract_expr(st, ctx, v, state, visited_fns);
                }
            }
            Expr::FnLit { params, body, .. } => {
                let mut cl_shapes = ctx.local_shapes.clone();
                let shadowed: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
                for p in params { cl_shapes.insert(p.name.clone(), self.shape_of_type(&p.ty)); }
                let cl_protects: Vec<String> = ctx.protects.iter().filter(|p| !shadowed.contains(path_root(p))).cloned().collect();
                if !cl_protects.is_empty() {
                    let roots = cl_protects.iter().map(|p| path_root(p).to_string()).collect();
                    let mut cl_ctx = VerifyCtx::sub(ctx.mode.clone(), cl_protects, roots, cl_shapes, false);
                    let (cl_end, cl_div) = self.verify_contract_stmts(st, &mut cl_ctx, &body.stmts, LockState::Unlocked, visited_fns);
                    self.check_exit_unlocked(&cl_ctx, cl_end, cl_div, body.span, "closure exit");
                }
            }
        }
    }

    fn verify_path_read(&mut self, ctx: &VerifyCtx, path: &str, state: LockState, span: Span) {
        match classify_path(path, &ctx.mode, &ctx.protects, &ctx.shared_roots) {
            PathMatch::None => {}
            PathMatch::Guard => self.err(span, format!("mutex guard `{path}` cannot be read or copied as a value — use `.lock()`/`.unlock()` or pass `&{path}` to a checked `task` (HYB-E0005)")),
            PathMatch::UncoveredSiblingField => self.err_note(span, format!("read of `{path}` touches shared root `{}` outside declared contract access set `protects: [{}]` (HYB-E0005)", path_root(path), ctx.protects.join(", ")), format!("the contract only covers `[{}]`; add `{path}` (or `{}`) to `protects:` and synchronize its accesses", ctx.protects.join(", "), path_root(path))),
            PathMatch::Protected => match &ctx.mode {
                ContractMode::Atomic => self.err_note(span, format!("raw non-atomic read of `{path}` violates `\"atomic\"` contract — read via `atomic.load_*(&{path})` or `{path}.load()` (HYB-E0005)"), format!("all reads and writes of `{path}` must go through `atomic.*` operations")),
                ContractMode::Mutex { guard_path, .. } if state == LockState::Unlocked => self.err_note(span, format!("unguarded read of `{path}` outside `{guard_path}.lock()` ... `{guard_path}.unlock()` critical section (HYB-E0005)"), format!("acquire `{guard_path}.lock()` (or `{guard_path}.rlock()`) before reading `{path}`")),
                ContractMode::Mutex { .. } => {}
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_contract_call(&mut self, st: &FnState, ctx: &mut VerifyCtx, callee: &Expr, args: &[CallArg], state: LockState, span: Span, visited_fns: &mut HashSet<String>) -> bool {
        let callee_unwrapped = unwrap_parens(callee);
        if matches!(ctx.mode, ContractMode::Atomic) {
            if let Expr::Field { base, name, .. } = callee_unwrapped {
                if matches!(unwrap_parens(base), Expr::Ident { name: pkg, .. } if pkg == "atomic") && is_atomic_pkg_op(name) {
                    if let Some(first) = args.first() {
                        if let Some(p) = expr_dotted_path(as_addr_of(&first.value).unwrap_or(&first.value)) {
                            if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                                for rest in &args[1..] { self.verify_contract_expr(st, ctx, &rest.value, state, visited_fns); }
                                return true;
                            }
                        }
                    }
                }
                if is_atomic_method_op(name) {
                    if let Some(p) = expr_dotted_path(base) {
                        if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                            for a in args { self.verify_contract_expr(st, ctx, &a.value, state, visited_fns); }
                            return true;
                        }
                    }
                }
            }
        }

        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
            if let Expr::Ident { name, .. } = callee_unwrapped {
                if matches!(name.as_str(), "len" | "cap") && args.len() == 1 {
                    if let Some(p) = expr_dotted_path(&args[0].value) {
                        if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                            self.verify_path_read(ctx, &p, state, span);
                            return true;
                        }
                    }
                }
                if matches!(name.as_str(), "delete" | "clear") && !args.is_empty() {
                    if let Some(p) = expr_dotted_path(&args[0].value) {
                        if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                            if state != LockState::WriteLocked {
                                self.err_note(span, format!("`{name}` mutates `{p}` outside exclusive `{guard_path}.lock()` critical section (HYB-E0005)"), format!("acquire `{guard_path}.lock()` before calling `{name}` on `{p}`"));
                            }
                            for rest in &args[1..] { self.verify_contract_expr(st, ctx, &rest.value, state, visited_fns); }
                            return true;
                        }
                    }
                }
            }
        }

        let passes_protected_ref = args.iter().any(|a| {
            let v = unwrap_parens(&a.value);
            if let Some(inner) = as_addr_of(v) {
                if let Some(p) = expr_dotted_path(inner) {
                    return classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) != PathMatch::None;
                }
            }
            if let Some(p) = expr_dotted_path(v) {
                if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                    return self.expr_shape_in_ctx(st, ctx, v).is_reference_alias() || matches!(ctx.mode, ContractMode::Atomic);
                }
            }
            false
        });

        if passes_protected_ref {
            if let Expr::Ident { name: fn_name, .. } = callee_unwrapped {
                if let Some(fn_decl) = self.find_fn_decl(fn_name) {
                    if state != LockState::Unlocked {
                        if let ContractMode::Mutex { guard_path, .. } = &ctx.mode {
                            self.err(span, format!("cannot pass protected state to `{fn_name}` while `{guard_path}` is already locked — either inline the access or call `{fn_name}` unlocked so its contract can be checked (HYB-E0005)"));
                            return true;
                        }
                    }
                    self.verify_callee_fn(st, ctx, fn_decl, args, span, visited_fns);
                    return true;
                }
            }
            self.err_note(span, format!("cannot pass contract-protected reference directly to unverified call `{}` — external callees may retain aliases or access state outside the contract (HYB-E0005)", callee_unwrapped.render()), "read the needed primitive/string fields inside the critical section first, then pass those copies to the call");
            return true;
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_task_spawn(&mut self, st: &FnState, ctx: &mut VerifyCtx, callee: &Expr, args: &[CallArg], state: LockState, span: Span, visited_fns: &mut HashSet<String>) {
        for a in args {
            let raw = unwrap_parens(&a.value);
            if matches!(raw, Expr::Move { .. }) { continue; }
            if let Some(inner) = as_addr_of(raw) {
                if let Some(p) = expr_dotted_path(inner) {
                    let m = classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots);
                    if m == PathMatch::Guard || m == PathMatch::Protected || ctx.shared_roots.contains(path_root(&p)) { continue; }
                    if !matches!(self.expr_shape_in_ctx(st, ctx, inner), TypeShape::Atomic) {
                        self.err_note(a.span, format!("shared borrow `&{p}` crosses task spawn inside a contracted `managed` block without being listed in `protects:` (HYB-E0005)"), format!("add `{p}` to `protects:` and synchronize every access, or remove it from this `managed` block"));
                        continue;
                    }
                }
            }
            if let Some(p) = expr_dotted_path(raw) {
                let m = classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots);
                if m == PathMatch::Guard {
                    if !matches!(self.expr_shape_in_ctx(st, ctx, raw), TypeShape::Pointer(_)) {
                        self.err_note(a.span, format!("mutex guard `{p}` passed by value at task spawn — copying a `sync.mutex` creates an independent lock per task; pass `&{p}` instead (HYB-E0005)"), format!("pass `&{p}` and change the worker parameter type to `*sync.mutex`"));
                    }
                    continue;
                }
                if m == PathMatch::Protected { continue; }
                let sh = self.expr_shape_in_ctx(st, ctx, raw);
                if !sh.is_sendable_by_type() {
                    self.err_note(a.span, format!("non-sendable argument `{p}` ({}) escapes at task spawn but is not covered by `protects: [{}]` (HYB-E0005)", sh.render(), ctx.protects.join(", ")), format!("every shared mutable value escaping through a contracted `managed` block must be covered by `protects:` (or transferred with `move {p}`)"));
                    continue;
                }
            }
            self.verify_contract_expr(st, ctx, raw, state, visited_fns);
        }

        let touches_contract = args.iter().any(|a| {
            let v = unwrap_parens(&a.value);
            expr_dotted_path(as_addr_of(v).unwrap_or(v)).map(|p| classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) != PathMatch::None || ctx.shared_roots.contains(path_root(&p))).unwrap_or(false)
        });
        if !touches_contract { return; }

        let callee_unwrapped = unwrap_parens(callee);
        let Expr::Ident { name: fn_name, .. } = callee_unwrapped else {
            self.err(span, format!("cannot verify `managed` contract across non-static task callee `{}` — spawn a top-level `fn` so its synchronization can be checked (HYB-E0005)", callee_unwrapped.render()));
            return;
        };
        let Some(fn_decl) = self.find_fn_decl(fn_name) else {
            self.err(span, format!("cannot verify `managed` contract across unresolved task callee `{fn_name}` (HYB-E0005)"));
            return;
        };
        self.verify_callee_fn(st, ctx, fn_decl, args, span, visited_fns);
    }

    fn verify_callee_fn(&mut self, _st: &FnState, caller_ctx: &VerifyCtx, fn_decl: &FnDecl, args: &[CallArg], call_span: Span, visited_fns: &mut HashSet<String>) {
        let Some(fn_body) = &fn_decl.body else {
            self.err(call_span, format!("cannot verify `managed` contract on bodyless function `{}` (HYB-E0005)", fn_decl.name));
            return;
        };
        let mut mapped_protects = Vec::new();
        let mut mapped_roots = HashSet::new();
        let mut mapped_guard: Option<(String, bool)> = None;

        for (arg, param) in args.iter().zip(fn_decl.params.iter()) {
            let v = unwrap_parens(&arg.value);
            if matches!(v, Expr::Move { .. }) { continue; }
            let Some(arg_path) = expr_dotted_path(as_addr_of(v).unwrap_or(v)) else { continue; };
            if let ContractMode::Mutex { guard_path, is_rw } = &caller_ctx.mode {
                if let Some(remapped) = remap_path(guard_path, &arg_path, &param.name) { mapped_guard = Some((remapped, *is_rw)); }
            }
            for prot in &caller_ctx.protects {
                if let Some(remapped) = remap_path(prot, &arg_path, &param.name) {
                    mapped_roots.insert(path_root(&remapped).to_string());
                    mapped_protects.push(remapped);
                }
            }
            if caller_ctx.shared_roots.contains(&arg_path) { mapped_roots.insert(param.name.clone()); }
        }
        if mapped_protects.is_empty() && mapped_roots.is_empty() { return; }

        let callee_mode = match &caller_ctx.mode {
            ContractMode::Atomic => ContractMode::Atomic,
            ContractMode::Mutex { guard_path, is_rw } => match mapped_guard {
                Some((g, rw)) => ContractMode::Mutex { guard_path: g, is_rw: rw },
                None => ContractMode::Mutex { guard_path: format!("<unpassed caller guard `{guard_path}`>"), is_rw: *is_rw },
            },
        };
        let visit_key = format!("{}::{callee_mode:?}::{mapped_protects:?}", fn_decl.name);
        if !visited_fns.insert(visit_key.clone()) { return; }

        let mut callee_shapes: HashMap<String, TypeShape> = self.consts.iter().map(|(k, b)| (k.clone(), b.shape.clone())).collect();
        for p in &fn_decl.params { callee_shapes.insert(p.name.clone(), self.shape_of_type(&p.ty)); }

        let mut callee_ctx = VerifyCtx::sub(callee_mode, mapped_protects, mapped_roots, callee_shapes, false);
        let callee_st = FnState::default();
        let (end_s, div) = self.verify_contract_stmts(&callee_st, &mut callee_ctx, &fn_body.stmts, LockState::Unlocked, visited_fns);
        self.check_exit_unlocked(&callee_ctx, end_s, div, fn_body.span, &format!("callee `{}` exit", fn_decl.name));
        visited_fns.remove(&visit_key);
    }

    #[allow(clippy::too_many_arguments)]
    fn verify_channel_send(&mut self, st: &FnState, ctx: &mut VerifyCtx, value: &Expr, is_move: bool, state: LockState, span: Span, visited_fns: &mut HashSet<String>) {
        if is_move || matches!(unwrap_parens(value), Expr::Move { .. }) { return; }
        let unwrapped = unwrap_parens_and_move(value);
        if let Some(inner) = as_addr_of(unwrapped) {
            if let Some(p) = expr_dotted_path(inner) {
                self.err_note(span, format!("cannot send address `&{p}` over a channel inside a contracted `managed` block — channel receivers are not bound to the block's synchronization contract (HYB-E0005)"), "send a value copy or `move` an owned value over the channel, or pass `&target` directly to a checked `task`");
                return;
            }
        }
        if let Some(p) = expr_dotted_path(unwrapped) {
            let sh = self.expr_shape_in_ctx(st, ctx, unwrapped);
            if !sh.is_sendable_by_type() {
                self.err_note(span, format!("non-sendable `{p}` ({}) cannot cross a channel send inside a contracted `managed` block without `move` — channel consumers cannot be verified against the block's contract (HYB-E0005)", sh.render()), format!("send an owned transfer (`ch <- move {p}`) or send a sendable value copy computed inside the critical section"));
                return;
            }
        }
        self.verify_contract_expr(st, ctx, value, state, visited_fns);
    }

    fn check_no_protected_alias(&mut self, st: &FnState, ctx: &VerifyCtx, expr: &Expr, span: Span) {
        let e = unwrap_parens(expr);
        if let Some(inner) = as_addr_of(e) {
            if let Some(p) = expr_dotted_path(inner) {
                if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) != PathMatch::None
                    || ctx.shared_roots.contains(path_root(&p))
                {
                    self.err(span, format!("cannot store address `&{p}` into an alias — contract-protected state must be accessed directly through its declared name (HYB-E0005)"));
                }
            }
            return;
        }
        if let Some(p) = expr_dotted_path(e) {
            if classify_path(&p, &ctx.mode, &ctx.protects, &ctx.shared_roots) == PathMatch::Protected {
                if self.shape_may_share(&self.expr_shape_in_ctx(st, ctx, e)) {
                    self.err(span, format!("cannot alias contract-protected reference `{}` into another binding — access `{p}` directly so every operation is checked against the contract (HYB-E0005)", e.render()));
                }
            }
        }
    }

    fn try_mutex_transition(&mut self, ctx: &VerifyCtx, expr: &Expr, state: LockState, span: Span) -> Option<LockState> {
        let ContractMode::Mutex { guard_path, is_rw } = &ctx.mode else { return None; };
        let Expr::Call { callee, args, .. } = unwrap_parens(expr) else { return None; };
        if !args.is_empty() { return None; }
        let Expr::Field { base, name, .. } = unwrap_parens(callee) else { return None; };
        if expr_dotted_path(base).as_deref() != Some(guard_path.as_str()) { return None; }
        match name.as_str() {
            "lock" => {
                if state != LockState::Unlocked {
                    self.err_note(span, format!("double lock on `{guard_path}` inside `managed` mutex contract (HYB-E0005)"), format!("unlock `{guard_path}` before acquiring it again"));
                }
                Some(LockState::WriteLocked)
            }
            "unlock" => {
                if state != LockState::WriteLocked {
                    self.err_note(span, format!("`{guard_path}.unlock()` called when `{guard_path}` is not exclusively locked (HYB-E0005)"), format!("call `{guard_path}.lock()` before `{guard_path}.unlock()`"));
                }
                Some(LockState::Unlocked)
            }
            "rlock" => {
                if !is_rw {
                    self.err_note(span, format!("`{guard_path}.rlock()` requires `sync.rw_mutex`, but `{guard_path}` is `sync.mutex` (HYB-E0005)"), format!("use `{guard_path}.lock()` or change `{guard_path}` to `sync.rw_mutex`"));
                } else if state != LockState::Unlocked {
                    self.err(span, format!("`{guard_path}.rlock()` called while `{guard_path}` is already locked (HYB-E0005)"));
                }
                Some(LockState::ReadLocked)
            }
            "runlock" => {
                if state != LockState::ReadLocked {
                    self.err(span, format!("`{guard_path}.runlock()` called without a matching `{guard_path}.rlock()` (HYB-E0005)"));
                }
                Some(LockState::Unlocked)
            }
            _ => None,
        }
    }

    fn try_defer_unlock(&mut self, ctx: &VerifyCtx, call: &Expr, state: LockState, span: Span) -> bool {
        let ContractMode::Mutex { guard_path, .. } = &ctx.mode else { return false; };
        let Expr::Call { callee, args, .. } = unwrap_parens(call) else { return false; };
        if !args.is_empty() { return false; }
        let Expr::Field { base, name, .. } = unwrap_parens(callee) else { return false; };
        if expr_dotted_path(base).as_deref() != Some(guard_path.as_str()) || !matches!(name.as_str(), "unlock" | "runlock") {
            return false;
        }
        if ctx.in_loop {
            self.err_note(
                span,
                format!("`defer {guard_path}.{name}()` inside a loop defers unlock until function exit rather than iteration end (HYB-E0005)"),
                format!("call `{guard_path}.{name}()` explicitly at the end of the loop iteration"),
            );
            return true;
        }
        let ok = (name == "unlock" && state == LockState::WriteLocked) || (name == "runlock" && state == LockState::ReadLocked);
        if !ok {
            self.err(span, format!("`defer {guard_path}.{name}()` without holding the matching lock on `{guard_path}` (HYB-E0005)"));
        }
        true
    }

    fn collect_scope_shapes(&self, st: &FnState) -> HashMap<String, TypeShape> {
        let mut out: HashMap<String, TypeShape> = self.consts.iter().map(|(k, b)| (k.clone(), b.shape.clone())).collect();
        for (k, b) in &st.env {
            out.insert(k.clone(), b.shape.clone());
        }
        for scope in &st.scopes {
            for (k, b) in scope {
                out.insert(k.clone(), b.shape.clone());
            }
        }
        out
    }

    fn record_local_decl_shapes(&self, st: &FnState, d: &LocalDecl, shapes: &mut HashMap<String, TypeShape>) {
        for (i, name) in d.names.iter().enumerate() {
            if name == "_" { continue; }
            let sh = if let Some(ty) = &d.ty {
                self.shape_of_type(ty)
            } else if let Some(val) = d.values.get(i) {
                self.infer_expr_shape_with_map(st, shapes, val)
            } else {
                TypeShape::Unknown
            };
            shapes.insert(name.clone(), sh);
        }
    }

    fn expr_shape_in_ctx(&self, st: &FnState, ctx: &VerifyCtx, expr: &Expr) -> TypeShape {
        self.infer_expr_shape_with_map(st, &ctx.local_shapes, expr)
    }

    fn infer_expr_shape_with_map(&self, st: &FnState, shapes: &HashMap<String, TypeShape>, expr: &Expr) -> TypeShape {
        let expr = unwrap_parens(expr);
        match expr {
            Expr::Ident { name, .. } => shapes.get(name).cloned().unwrap_or_else(|| self.shape_of_expr(st, expr)),
            Expr::Field { base, name, .. } => self.field_shape(&self.infer_expr_shape_with_map(st, shapes, base), name),
            Expr::AddrOf { operand, .. } | Expr::Unary { op: hy_syntax::UnOp::Addr, operand, .. } => {
                TypeShape::Pointer(Box::new(self.infer_expr_shape_with_map(st, shapes, operand)))
            }
            Expr::Index { base, .. } => match self.infer_expr_shape_with_map(st, shapes, base) {
                TypeShape::Map(_, v) => *v,
                TypeShape::Slice(e) | TypeShape::Array(e) => *e,
                TypeShape::Str => TypeShape::Primitive,
                _ => self.shape_of_expr(st, expr),
            },
            Expr::Receive { chan, .. } => match self.infer_expr_shape_with_map(st, shapes, chan) {
                TypeShape::Chan(e) => *e,
                _ => self.shape_of_expr(st, expr),
            },
            _ => self.shape_of_expr(st, expr),
        }
    }

    fn resolve_contract_path(&self, shapes: &HashMap<String, TypeShape>, path: &str) -> Option<TypeShape> {
        let mut segs = path.split('.');
        let mut cur = shapes.get(segs.next()?)?.clone();
        for field in segs {
            if !self.shape_has_struct_field(&cur, field) { return None; }
            cur = self.field_shape(&cur, field);
        }
        Some(cur)
    }

    fn resolve_contract_path_ext(&self, st: &FnState, body_stmts: &[Stmt], shapes: &HashMap<String, TypeShape>, path: &str) -> Option<(TypeShape, Option<TypeExpr>)> {
        let shape = self.resolve_contract_path(shapes, path)?;
        let ty = if !path.contains('.') {
            self.lookup_binding(st, path).and_then(|b| b.ty.clone()).or_else(|| {
                body_stmts.iter().find_map(|s| match s {
                    Stmt::Local(d) if d.names.iter().any(|n| n == path) => d.ty.clone(),
                    _ => None,
                })
            })
        } else {
            None
        };
        Some((shape, ty))
    }

    fn shape_has_struct_field(&self, shape: &TypeShape, field: &str) -> bool {
        let sname = match shape {
            TypeShape::Struct(Some(s)) => Some(s.as_str()),
            TypeShape::Pointer(inner) => match &**inner {
                TypeShape::Struct(Some(s)) => Some(s.as_str()),
                _ => None,
            },
            _ => None,
        };
        sname.and_then(|s| self.struct_fields_for_sname(s)).map(|fs| fs.iter().any(|(f, _)| f == field)).unwrap_or(false)
    }

    fn find_fn_decl(&self, name: &str) -> Option<&'a FnDecl> {
        self.file.decls.iter().find_map(|d| match d {
            Decl::Fn(f) if f.receiver.is_none() && f.name == name => Some(f),
            _ => None,
        })
    }

    fn err(&mut self, span: Span, msg: impl Into<String>) {
        self.emit_contract_err(span, msg.into(), Vec::new());
    }

    fn err_note(&mut self, span: Span, msg: impl Into<String>, note: impl Into<String>) {
        self.emit_contract_err(span, msg.into(), vec![note.into()]);
    }

    fn emit_contract_err(&mut self, span: Span, msg: String, notes: Vec<String>) {
        self.diags.push(Diagnostic::error("HYB-E0005", self.path.clone(), span, msg).with_notes(notes));
    }
}

fn path_root(path: &str) -> &str {
    path.split('.').next().unwrap_or(path)
}

fn remap_path(contract_path: &str, arg_path: &str, param_name: &str) -> Option<String> {
    if contract_path == arg_path {
        Some(param_name.to_string())
    } else {
        contract_path.strip_prefix(arg_path).and_then(|r| r.strip_prefix('.')).map(|suf| format!("{param_name}.{suf}"))
    }
}

fn classify_path(path: &str, mode: &ContractMode, protects: &[String], shared_roots: &HashSet<String>) -> PathMatch {
    if let ContractMode::Mutex { guard_path, .. } = mode {
        if path == guard_path || path.starts_with(&format!("{guard_path}.")) {
            return PathMatch::Guard;
        }
    }
    for p in protects {
        if path == p || path.starts_with(&format!("{p}.")) {
            return PathMatch::Protected;
        }
        if p.starts_with(&format!("{path}.")) {
            return PathMatch::UncoveredSiblingField;
        }
    }
    if shared_roots.contains(path_root(path)) {
        return PathMatch::UncoveredSiblingField;
    }
    PathMatch::None
}

fn expr_dotted_path(expr: &Expr) -> Option<String> {
    match unwrap_parens(expr) {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Field { base, name, .. } => Some(format!("{}.{name}", expr_dotted_path(base)?)),
        Expr::Index { base, .. } | Expr::Slice { base, .. } => expr_dotted_path(base),
        _ => None,
    }
}

fn place_dotted_path(place: &Place) -> Option<String> {
    match &place.kind {
        PlaceKind::Ident(n) => Some(n.clone()),
        PlaceKind::Field { base, name } => Some(format!("{}.{name}", place_dotted_path(base)?)),
        PlaceKind::Index { base, .. } | PlaceKind::AddrOf { base } => place_dotted_path(base),
    }
}

fn is_mutex_shape(shape: &TypeShape) -> (bool, bool) {
    let inner = match shape {
        TypeShape::Pointer(inner) => inner.as_ref(),
        other => other,
    };
    match inner {
        TypeShape::ForeignOpaque(name) => match name.as_str() {
            "sync.mutex" | "sync.Mutex" => (true, false),
            "sync.rw_mutex" | "sync.RWMutex" => (true, true),
            _ => (false, false),
        },
        _ => (false, false),
    }
}

fn is_valid_atomic_target(shape: &TypeShape, ty: Option<&TypeExpr>) -> bool {
    match shape {
        TypeShape::Atomic | TypeShape::Primitive => true,
        TypeShape::Pointer(inner) => matches!(**inner, TypeShape::Atomic | TypeShape::Primitive | TypeShape::Unknown),
        TypeShape::Unknown => ty
            .map(|t| match t {
                TypeExpr::Named { name, .. } => {
                    let base = name.split('[').next().unwrap_or(name);
                    foreign::is_scalar_atomic(base) || foreign::is_unproven_atomic(base)
                }
                _ => false,
            })
            .unwrap_or(false),
        _ => false,
    }
}

fn is_atomic_pkg_op(name: &str) -> bool {
    let l = name.to_ascii_lowercase();
    l.starts_with("load") || l.starts_with("store") || l.starts_with("add") || l.starts_with("swap") || l.starts_with("compare_and_swap") || l.starts_with("compareandswap")
}

fn is_atomic_method_op(name: &str) -> bool {
    matches!(name.to_ascii_lowercase().as_str(), "load" | "store" | "add" | "swap" | "compare_and_swap" | "compareandswap")
}

fn iterate_var_shape(iter_shape: &TypeShape, idx: usize, total: usize) -> TypeShape {
    match iter_shape {
        TypeShape::Chan(elem) => if idx == 0 { (**elem).clone() } else { TypeShape::Primitive },
        TypeShape::Slice(elem) | TypeShape::Array(elem) => if total == 1 || idx > 0 { (**elem).clone() } else { TypeShape::Primitive },
        TypeShape::Map(k, v) => if idx == 0 { (**k).clone() } else { (**v).clone() },
        TypeShape::Str => TypeShape::Primitive,
        _ => TypeShape::Unknown,
    }
}
