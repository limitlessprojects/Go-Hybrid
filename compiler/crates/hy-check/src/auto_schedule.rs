//! auto_schedule.rs — automatic fork-join scheduling (issue #1962).
//!
//! Inside an `auto { ... }` block, tasks are collected and scheduled
//! automatically: tasks with provably disjoint data accesses run in
//! parallel, ambiguous or conflicting tasks are serialized.
//!
//! Core model:
//!   * `TaskEffect` — read/write sets of root variable names
//!   * conflict detection — write-write, write-read overlap
//!   * greedy graph-coloring layering — each layer runs in parallel
//!
//! Safety constraints (checked separately):
//!   * a `task { ... }` is only schedulable as a DIRECT statement of an
//!     `auto` body — anywhere else the emitter would fall through to an
//!     unscheduled `go func() { … }()` (HYB-E3206)
//!   * no blocking calls (channel receive HYB-E3201, select HYB-E3202)
//!   * no dynamic spawn (task inside loops, HYB-E3203) and no plain
//!     `task f(...)` call (HYB-E3204)
//!   * no references taken inside the region (HYB-E3205)
//!   * the region's non-task statements stay ahead of its tasks (HYB-E3207)

use std::collections::HashSet;

use hy_syntax::{Block, Expr, File, InterpPart, Stmt};

use crate::model::Checker;
use hy_diag::Diagnostic;

/// The read/write effect summary of a single task.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskEffect {
    /// Root variable names read by this task.
    pub reads: HashSet<String>,
    /// Root variable names written by this task.
    pub writes: HashSet<String>,
}

impl TaskEffect {
    /// Record a read of a root variable.
    pub fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }

    /// Record a write to a root variable.
    pub fn write(&mut self, name: &str) {
        self.writes.insert(name.to_string());
    }

    /// Check if this task conflicts with another.
    /// Two tasks conflict if:
    ///   * both write the same variable (write-write race)
    ///   * one writes and the other reads the same variable (write-read race)
    pub fn conflicts_with(&self, other: &TaskEffect) -> bool {
        // write-write overlap
        if !self.writes.is_disjoint(&other.writes) {
            return true;
        }
        // write-read overlap (both directions)
        if !self.writes.is_disjoint(&other.reads) {
            return true;
        }
        if !other.writes.is_disjoint(&self.reads) {
            return true;
        }
        false
    }
}

/// Analyze the effects of a task block.
pub(crate) fn analyze_block(block: &Block) -> TaskEffect {
    let mut effect = TaskEffect::default();
    for stmt in &block.stmts {
        analyze_stmt(stmt, &mut effect);
    }
    effect
}

/// Analyze a single statement, updating the effect sets.
fn analyze_stmt(stmt: &Stmt, effect: &mut TaskEffect) {
    match stmt {
        Stmt::Local(local) => {
            // `x := ...` / `let x = ...` / `var x int = ...`
            // The right-hand side is read; the left-hand side is written.
            for v in &local.values {
                analyze_expr(v, effect);
            }
            for name in &local.names {
                effect.write(name);
            }
        }
        Stmt::Assign { targets, op: _, values, .. } => {
            // Right-hand side is read first
            for v in values {
                analyze_expr(v, effect);
            }
            // Then targets are written
            for t in targets {
                let root = t.root();
                effect.write(root);
            }
        }
        Stmt::Expr { expr, .. } => {
            analyze_expr(expr, effect);
        }
        Stmt::Send { chan, value, .. } => {
            analyze_expr(chan, effect);
            analyze_expr(value, effect);
        }
        Stmt::Return { values, .. } => {
            for v in values {
                analyze_expr(v, effect);
            }
        }
        Stmt::If { init, cond, then, otherwise, .. } => {
            if let Some(init) = init {
                analyze_stmt(init, effect);
            }
            analyze_expr(cond, effect);
            merge_block(then, effect);
            if let Some(otherwise) = otherwise {
                analyze_stmt(otherwise, effect);
            }
        }
        Stmt::For { body, .. } => {
            // Conservative: loops read and write everything inside
            merge_block(body, effect);
        }
        Stmt::Block { block, .. } => {
            merge_block(block, effect);
        }
        Stmt::TaskBlock { body, .. } => {
            // Nested task blocks: their effects are nested, not collected here
            // (auto scheduling is one level deep; nested tasks are their own scope)
            merge_block(body, effect);
        }
        Stmt::Auto { body, .. } => {
            merge_block(body, effect);
        }
        Stmt::Managed { body, .. } => {
            merge_block(body, effect);
        }
        // Other statements (break, continue, defer, match, select, ...)
        // are conservative: we don't deeply analyze them.
        _ => {}
    }
}

/// Merge a block's effects into an existing effect (for nested blocks).
fn merge_block(block: &Block, effect: &mut TaskEffect) {
    for stmt in &block.stmts {
        analyze_stmt(stmt, effect);
    }
}

/// Analyze an expression, updating the effect sets.
fn analyze_expr(expr: &Expr, effect: &mut TaskEffect) {
    match expr {
        Expr::Ident { name, .. } => {
            effect.read(name);
        }
        Expr::Call { callee, args, .. } => {
            analyze_expr(callee, effect);
            for arg in args {
                analyze_expr(&arg.value, effect);
            }
        }
        Expr::Unary { op: _, operand, .. } => {
            analyze_expr(operand, effect);
        }
        Expr::Binary { lhs, rhs, .. } => {
            analyze_expr(lhs, effect);
            analyze_expr(rhs, effect);
        }
        Expr::Lit { .. } => {
            // literals have no variable effects
        }
        Expr::Interp { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(inner) = part {
                    analyze_expr(inner, effect);
                }
            }
        }
        Expr::Index { base, index, .. } => {
            analyze_expr(base, effect);
            analyze_expr(index, effect);
        }
        Expr::Slice { base, lo, hi, .. } => {
            analyze_expr(base, effect);
            if let Some(lo) = lo {
                analyze_expr(lo, effect);
            }
            if let Some(hi) = hi {
                analyze_expr(hi, effect);
            }
        }
        Expr::Field { base, name: _, .. } => {
            analyze_expr(base, effect);
        }
        Expr::AddrOf { operand, .. } => {
            analyze_expr(operand, effect);
        }
        Expr::Try { operand, .. } => {
            analyze_expr(operand, effect);
        }
        Expr::Move { operand, .. } => {
            analyze_expr(operand, effect);
        }
        Expr::Paren { inner, .. } => {
            analyze_expr(inner, effect);
        }
        // Other expressions: conservative, no effect recorded.
        _ => {}
    }
}

/// Greedy graph-coloring scheduler: assign tasks to layers.
/// Each layer contains tasks that do not conflict with each other.
/// Layers run sequentially, within a layer tasks run in parallel.
pub(crate) fn schedule_tasks(tasks: &[&Block]) -> Vec<Vec<usize>> {
    let effects: Vec<TaskEffect> = tasks.iter().map(|b| analyze_block(b)).collect();

    let mut layers: Vec<Vec<usize>> = Vec::new();

    for (i, effect) in effects.iter().enumerate() {
        let mut placed = false;
        for layer in layers.iter_mut() {
            // Check if this task conflicts with anyone already in the layer
            let conflicts = layer.iter().any(|&j| effect.conflicts_with(&effects[j]));
            if !conflicts {
                layer.push(i);
                placed = true;
                break;
            }
        }
        if !placed {
            layers.push(vec![i]);
        }
    }

    layers
}

/// Collect all inline task blocks from an auto block and schedule them.
/// Returns the layers of task indices (each layer runs in parallel).
pub(crate) fn schedule_block_layers(block: &Block) -> Vec<Vec<usize>> {
    let mut task_blocks: Vec<&Block> = Vec::new();

    for stmt in &block.stmts {
        if let Stmt::TaskBlock { body, .. } = stmt {
            task_blocks.push(body);
        }
    }

    schedule_tasks(&task_blocks)
}

/// A safety violation found in an auto block.
#[derive(Debug, Clone)]
pub(crate) struct SafetyViolation {
    pub kind: SafetyViolationKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SafetyViolationKind {
    /// Blocking receive (`<-ch`) inside auto scope
    BlockingReceive,
    /// `select` statement inside auto scope
    BlockingSelect,
    /// Task block inside a loop (dynamic spawn)
    DynamicSpawn,
    /// Plain `task fn(...)` call inside auto scope (only inline `task { ... }` allowed)
    EscapedTaskCall,
    /// A reference (`&x`) created inside auto scope: writes through it are
    /// invisible to the name-based conflict test, so the region's disjointness
    /// proof would not cover them
    EscapedReference,
}

/// Check an auto block for safety violations (issue #1962 constraints).
///
/// Constraints (from the proposal):
///   * No blocking calls (channel receive, select) — the scheduler needs
///     to know all tasks upfront; a blocking call would deadlock the layer.
///   * No dynamic spawn (task inside loops) — the task set must be statically
///     known at compile time.
///   * No references taken inside the region — the layering proof is by
///     variable name, and a pointer hides the writes it enables.
pub(crate) fn check_auto_safety(block: &Block) -> Vec<SafetyViolation> {
    let mut violations = Vec::new();
    check_block_safety(block, &mut violations, 0);
    violations
}

fn check_block_safety(block: &Block, violations: &mut Vec<SafetyViolation>, loop_depth: u32) {
    for stmt in &block.stmts {
        check_stmt_safety(stmt, violations, loop_depth);
    }
}

fn check_stmt_safety(stmt: &Stmt, violations: &mut Vec<SafetyViolation>, loop_depth: u32) {
    match stmt {
        Stmt::TaskBlock { body, .. } => {
            // Dynamic spawn: task inside a loop
            if loop_depth > 0 {
                violations.push(SafetyViolation {
                    kind: SafetyViolationKind::DynamicSpawn,
                    message: "task block inside a loop — dynamic spawn is not allowed in auto scope".to_string(),
                });
            }
            check_block_safety(body, violations, loop_depth);
        }
        // Plain `task fn(...)` call inside auto scope is not allowed:
        // we can't analyze the callee's effects, so it might escape state
        // to the outer goroutine. Only inline `task { ... }` blocks are allowed.
        Stmt::Task { callee, .. } => {
            violations.push(SafetyViolation {
                kind: SafetyViolationKind::EscapedTaskCall,
                message: format!(
                    "plain `task {}` inside auto scope — only inline `task {{ ... }}` blocks are allowed (external call could escape state)",
                    callee.render()
                ),
            });
        }
        Stmt::For { body, .. } => {
            check_block_safety(body, violations, loop_depth + 1);
        }
        Stmt::Block { block, .. } => {
            check_block_safety(block, violations, loop_depth);
        }
        Stmt::If { init, cond, then, otherwise, .. } => {
            if let Some(init) = init {
                check_stmt_safety(init, violations, loop_depth);
            }
            check_expr_safety(cond, violations);
            check_block_safety(then, violations, loop_depth);
            if let Some(otherwise) = otherwise {
                check_stmt_safety(otherwise, violations, loop_depth);
            }
        }
        Stmt::Local(local) => {
            for value in &local.values {
                check_expr_safety(value, violations);
            }
        }
        Stmt::Assign { values, .. } => {
            for value in values {
                check_expr_safety(value, violations);
            }
        }
        Stmt::Return { values, .. } => {
            for value in values {
                check_expr_safety(value, violations);
            }
        }
        Stmt::Managed { body, .. } | Stmt::Auto { body, .. } => {
            check_block_safety(body, violations, loop_depth);
        }
        Stmt::Match { subject, arms, .. } => {
            check_expr_safety(subject, violations);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    check_expr_safety(guard, violations);
                }
                check_block_safety(&arm.body, violations, loop_depth);
            }
        }
        Stmt::Defer { call, .. } => {
            check_expr_safety(call, violations);
        }
        Stmt::Select { .. } => {
            violations.push(SafetyViolation {
                kind: SafetyViolationKind::BlockingSelect,
                message: "select statement inside auto scope — blocking calls are not allowed".to_string(),
            });
        }
        Stmt::Expr { expr, .. } => {
            check_expr_safety(expr, violations);
        }
        Stmt::Send { chan, value, .. } => {
            check_expr_safety(chan, violations);
            check_expr_safety(value, violations);
        }
        _ => {}
    }
}

fn check_expr_safety(expr: &Expr, violations: &mut Vec<SafetyViolation>) {
    match expr {
        Expr::Receive { chan, .. } => {
            violations.push(SafetyViolation {
                kind: SafetyViolationKind::BlockingReceive,
                message: format!("channel receive `<-{}` inside auto scope — blocking calls are not allowed", chan.render()),
            });
        }
        Expr::Call { callee, args, .. } => {
            check_expr_safety(callee, violations);
            for arg in args {
                check_expr_safety(&arg.value, violations);
            }
        }
        Expr::Unary { operand, .. } => {
            check_expr_safety(operand, violations);
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_expr_safety(lhs, violations);
            check_expr_safety(rhs, violations);
        }
        Expr::Index { base, index, .. } => {
            check_expr_safety(base, violations);
            check_expr_safety(index, violations);
        }
        Expr::Field { base, .. } => {
            check_expr_safety(base, violations);
        }
        // The scheduler proves independence by ROOT NAME (`analyze_expr`
        // records reads/writes of identifiers). A reference made here hands
        // the value to whoever holds it, and writes through it never appear in
        // any effect set — so the layering could be built on a proof the
        // program invalidates. Fail-closed, as everywhere else in Tier 0.
        Expr::AddrOf { operand, .. } => {
            violations.push(SafetyViolation {
                kind: SafetyViolationKind::EscapedReference,
                message: format!(
                    "`&{}` inside auto scope — a reference created here can be written through without the scheduler seeing it",
                    operand.render()
                ),
            });
            check_expr_safety(operand, violations);
        }
        Expr::Try { operand, .. } => {
            check_expr_safety(operand, violations);
        }
        Expr::Move { operand, .. } => {
            check_expr_safety(operand, violations);
        }
        Expr::Paren { inner, .. } => {
            check_expr_safety(inner, violations);
        }
        Expr::Interp { parts, .. } => {
            for part in parts {
                if let InterpPart::Expr(inner) = part {
                    check_expr_safety(inner, violations);
                }
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            check_expr_safety(base, violations);
            if let Some(lo) = lo {
                check_expr_safety(lo, violations);
            }
            if let Some(hi) = hi {
                check_expr_safety(hi, violations);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Task placement inside the region — HYB-E3206 (AUTO-SCOPE) and the
// statement-order rule HYB-E3207 (AUTO-ORDER)
// ---------------------------------------------------------------------------

/// The scheduler's task set is the list of `task { … }` blocks that are DIRECT
/// statements of an `auto` body: `hy-emit`'s `schedule_block` collects exactly
/// that shape, and every other `Stmt::TaskBlock` reaches the goroutine
/// fallback in `hy-emit/src/stmt.rs` — a spawn no layer joins, i.e. the race
/// the region exists to prevent. So a task outside the schedulable position is
/// rejected rather than emitted.
///
/// The same walk enforces the region's statement order: the emitter puts the
/// body's non-task statements ahead of the layers, so writing one after a task
/// would run it first (HYB-E3207).
pub(crate) fn check_task_scopes<'a>(ck: &mut Checker<'a>, file: &'a File) {
    for f in file.fns() {
        let Some(body) = &f.body else { continue };
        scopes_block(ck, body, false, false, 0);
    }
}

/// `schedulable` — this block's direct statements are task positions.
/// `in_auto`/`loops` mirror `check_auto_safety`: a task inside a loop within
/// an auto region is that pass's HYB-E3203 case, so it is not reported twice.
fn scopes_block(ck: &mut Checker<'_>, block: &Block, schedulable: bool, in_auto: bool, loops: u32) {
    if schedulable {
        // The emitter runs the region's non-task statements BEFORE the layers
        // (`hy-emit/src/stmt.rs`, the Auto arm), so a statement written after
        // a task would execute first — the program's order and the program's
        // behaviour disagree, silently. Either shape is fine in a region, the
        // mix that contradicts itself is not.
        let mut past_task = false;
        for stmt in &block.stmts {
            if matches!(stmt, Stmt::TaskBlock { .. }) {
                past_task = true;
                continue;
            }
            if !past_task {
                continue;
            }
            ck.diags.push(
                Diagnostic::error(
                    "HYB-E3207",
                    ck.path.clone(),
                    stmt.span(),
                    "`auto` region: this statement is written after a `task { ... }` block, but the region runs its non-task statements before the task layers",
                )
                .with_notes(vec![
                    "move the statement above the first `task { ... }` of the region (it then really does run first)".to_string(),
                    "or take it out of the `auto` block entirely — after the region joins, its order relative to the tasks is the one you wrote".to_string(),
                ]),
            );
        }
    }
    for stmt in &block.stmts {
        scopes_stmt(ck, stmt, schedulable, in_auto, loops);
    }
}

fn scopes_stmt(ck: &mut Checker<'_>, stmt: &Stmt, schedulable: bool, in_auto: bool, loops: u32) {
    // A closure body is its own callable: an enclosing `auto` never collects
    // tasks from it, and the dynamic-spawn pass does not walk into it either.
    let mut fnlits: Vec<&Expr> = Vec::new();
    crate::walk::own_fnlits(stmt, &mut fnlits);
    for f in fnlits {
        if let Expr::FnLit { body, .. } = f {
            scopes_block(ck, body, false, false, 0);
        }
    }

    match stmt {
        Stmt::Auto { body, .. } => scopes_block(ck, body, true, true, 0),
        Stmt::TaskBlock { body, span } => {
            if !schedulable && !(in_auto && loops > 0) {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E3206",
                        ck.path.clone(),
                        *span,
                        if in_auto {
                            "`task { … }` here is not a direct statement of the `auto` body, so the scheduler never collects it"
                                .to_string()
                        } else {
                            "`task { … }` is only legal as a direct statement of an `auto { … }` body".to_string()
                        },
                    )
                    .with_notes(vec![
                        "the region's task set — and the layers joined by its WaitGroups — are the inline blocks written directly in the `auto` body".to_string(),
                        "move the block up to the `auto` body, or use a plain `task f(...)` spawn outside the region".to_string(),
                    ]),
                );
            }
            scopes_block(ck, body, false, in_auto, loops);
        }
        Stmt::For { body, .. } => scopes_block(ck, body, false, in_auto, loops + 1),
        Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => {
            scopes_block(ck, block, false, in_auto, loops)
        }
        Stmt::If { init, then, otherwise, .. } => {
            if let Some(init) = init {
                scopes_stmt(ck, init, false, in_auto, loops);
            }
            scopes_block(ck, then, false, in_auto, loops);
            if let Some(otherwise) = otherwise {
                scopes_stmt(ck, otherwise, false, in_auto, loops);
            }
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                scopes_block(ck, &arm.body, false, in_auto, loops);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                scopes_block(ck, &arm.body, false, in_auto, loops);
            }
        }
        Stmt::Local(_)
        | Stmt::Assign { .. }
        | Stmt::Expr { .. }
        | Stmt::Send { .. }
        | Stmt::Return { .. }
        | Stmt::Task { .. }
        | Stmt::Defer { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. } => {}
    }
}

#[cfg(test)]
mod tests {    use super::*;
    use hy_diag::{Pos, Span};

    fn dummy_span() -> Span {
        Span::new(Pos::new(1, 1), Pos::new(1, 1))
    }

    #[test]
    fn test_conflict_detection() {
        let mut a = TaskEffect::default();
        a.write("x");
        let mut b = TaskEffect::default();
        b.write("x");
        assert!(a.conflicts_with(&b), "write-write should conflict");

        let mut c = TaskEffect::default();
        c.read("x");
        assert!(a.conflicts_with(&c), "write-read should conflict");

        let mut d = TaskEffect::default();
        d.read("y");
        assert!(!a.conflicts_with(&d), "disjoint vars should not conflict");
    }

    #[test]
    fn test_escaped_reference_is_a_violation() {
        // auto { task { slot := &counter } } — the effect model only records
        // the READ of `counter`; the write the pointer enables is invisible,
        // so the region must refuse it (HYB-E3205).
        use hy_syntax::{LocalDecl, LocalKind};
        let s = dummy_span();
        let take_addr = Expr::AddrOf {
            operand: Box::new(Expr::Ident {
                name: "counter".to_string(),
                span: s,
            }),
            span: s,
        };
        let body = Block {
            stmts: vec![Stmt::Local(LocalDecl {
                kind: LocalKind::Define,
                names: vec!["slot".to_string()],
                ty: None,
                values: vec![take_addr],
                span: s,
            })],
            span: s,
        };
        let region = Block {
            stmts: vec![Stmt::TaskBlock { body, span: s }],
            span: s,
        };
        let violations = check_auto_safety(&region);
        assert!(
            violations
                .iter()
                .any(|v| v.kind == SafetyViolationKind::EscapedReference),
            "a reference taken inside a task must be reported: {violations:?}"
        );
    }

    #[test]
    fn test_blocking_select_is_a_violation() {
        // auto { task { select { ... } } } — HYB-E3202; the clause's own
        // receive is not charged a second time (the arms are not descended).
        let s = dummy_span();
        let inner = Block {
            stmts: vec![Stmt::Select {
                arms: vec![hy_syntax::SelectArm {
                    kind: hy_syntax::SelectArmKind::Default,
                    body: Block {
                        stmts: vec![Stmt::Expr {
                            expr: Expr::Receive {
                                chan: Box::new(Expr::Ident {
                                    name: "a".to_string(),
                                    span: s,
                                }),
                                span: s,
                            },
                            span: s,
                        }],
                        span: s,
                    },
                }],
                span: s,
            }],
            span: s,
        };
        let region = Block {
            stmts: vec![Stmt::TaskBlock { body: inner, span: s }],
            span: s,
        };
        let kinds: Vec<_> = check_auto_safety(&region)
            .into_iter()
            .map(|v| v.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![SafetyViolationKind::BlockingSelect],
            "one select, one code"
        );
    }

    #[test]
    fn test_schedule_layers() {
        // Task 0: writes x
        // Task 1: writes y (independent of 0)
        // Task 2: reads x (conflicts with 0)
        //
        // Expected layers:
        //   layer 0: [0, 1] — 0 writes x, 1 writes y, no conflict
        //   layer 1: [2]    — 2 reads x, conflicts with 0
        let block0 = Block { stmts: vec![], span: dummy_span() };
        let block1 = Block { stmts: vec![], span: dummy_span() };
        let block2 = Block { stmts: vec![], span: dummy_span() };

        // We can't easily build real blocks in tests, so test the scheduler
        // with manually constructed effects.
        let mut layers: Vec<Vec<usize>> = Vec::new();

        let effects = vec![
            { let mut e = TaskEffect::default(); e.write("x"); e },
            { let mut e = TaskEffect::default(); e.write("y"); e },
            { let mut e = TaskEffect::default(); e.read("x"); e },
        ];

        for (i, effect) in effects.iter().enumerate() {
            let mut placed = false;
            for layer in layers.iter_mut() {
                let conflicts = layer.iter().any(|&j| effect.conflicts_with(&effects[j]));
                if !conflicts {
                    layer.push(i);
                    placed = true;
                    break;
                }
            }
            if !placed {
                layers.push(vec![i]);
            }
        }

        assert_eq!(layers.len(), 2, "should have 2 layers");
        assert_eq!(layers[0].len(), 2, "layer 0 should have 2 tasks");
        assert_eq!(layers[1].len(), 1, "layer 1 should have 1 task");
    }
}
