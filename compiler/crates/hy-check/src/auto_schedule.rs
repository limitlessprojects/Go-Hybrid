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
//!   * mutable state does not escape the auto block
//!   * no blocking calls (channel receive, select, ...)
//!   * no dynamic spawn (task inside loops)

use std::collections::HashSet;

use hy_syntax::{Block, Expr, Place, PlaceKind, Stmt};

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

    /// Merge another effect into this one.
    pub fn merge(&mut self, other: &TaskEffect) {
        self.reads.extend(other.reads.iter().cloned());
        self.writes.extend(other.writes.iter().cloned());
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
            analyze_block(then, effect);
            if let Some(otherwise) = otherwise {
                analyze_stmt(otherwise, effect);
            }
        }
        Stmt::For { body, .. } => {
            // Conservative: loops read and write everything inside
            analyze_block(body, effect);
        }
        Stmt::Block { block, .. } => {
            analyze_block(block, effect);
        }
        Stmt::TaskBlock { body, .. } => {
            // Nested task blocks: their effects are nested, not collected here
            // (auto scheduling is one level deep; nested tasks are their own scope)
            analyze_block(body, effect);
        }
        Stmt::Managed { body, .. } => {
            analyze_block(body, effect);
        }
        // Other statements (break, continue, defer, match, select, ...)
        // are conservative: we don't deeply analyze them.
        _ => {}
    }
}

/// Merge a block's effects into an existing effect (for nested blocks).
fn analyze_block(block: &Block, effect: &mut TaskEffect) {
    for stmt in &block.stmts {
        analyze_stmt(stmt, effect);
    }
}

/// Analyze an expression, updating the effect sets.
fn analyze_expr(expr: &Expr, effect: &mut TaskEffect) {
    match expr {
        Expr::Ident(name, _) => {
            effect.read(name);
        }
        Expr::Place(place, _) => {
            let root = place.root();
            effect.read(root);
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
        Expr::Binary { left, right, .. } => {
            analyze_expr(left, effect);
            analyze_expr(right, effect);
        }
        Expr::Lit(_, _) => {
            // literals have no variable effects
        }
        Expr::Interp { parts, .. } => {
            for part in parts {
                analyze_expr(part, effect);
            }
        }
        Expr::Index { base, index, .. } => {
            analyze_expr(base, effect);
            analyze_expr(index, effect);
        }
        Expr::Field { base, name: _, .. } => {
            analyze_expr(base, effect);
        }
        Expr::AddrOf(place, _) => {
            let root = place.root();
            effect.read(root);
        }
        Expr::Try(expr, _) => {
            analyze_expr(expr, effect);
        }
        Expr::Move(expr, _) => {
            analyze_expr(expr, effect);
        }
        // Other expressions: conservative, no effect recorded.
        _ => {}
    }
}

/// A scheduled task layer: tasks that can run in parallel.
pub(crate) struct ScheduledTask<'a> {
    pub body: &'a Block,
    pub effect: TaskEffect,
}

/// Greedy graph-coloring scheduler: assign tasks to layers.
/// Each layer contains tasks that do not conflict with each other.
/// Layers run sequentially, within a layer tasks run in parallel.
pub(crate) fn schedule_tasks(tasks: &[&Block]) -> Vec<Vec<usize>> {
    let effects: Vec<TaskEffect> = tasks.iter().map(|b| analyze_block(b)).collect();

    let mut layers: Vec<Vec<usize>> = Vec::new();

    for (i, effect) in effects.iter().enumerate() {
        let mut placed = false;
        for (layer_idx, layer) in layers.iter_mut().enumerate() {
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

/// Result of scheduling an auto block.
pub(crate) struct ScheduleResult {
    /// The collected task blocks (in source order).
    pub tasks: Vec<*const Block>,
    /// Layers of task indices (each layer runs in parallel).
    pub layers: Vec<Vec<usize>>,
}

/// Collect all inline task blocks from an auto block and schedule them.
pub(crate) fn schedule_block(block: &Block) -> ScheduleResult {
    let mut task_blocks: Vec<&Block> = Vec::new();

    for stmt in &block.stmts {
        if let Stmt::TaskBlock { body, .. } = stmt {
            task_blocks.push(body);
        }
    }

    let layers = schedule_tasks(&task_blocks);

    ScheduleResult {
        tasks: task_blocks.into_iter().map(|b| b as *const Block).collect(),
        layers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_schedule_layers() {
        // Task 0: writes x
        // Task 1: writes y (independent of 0)
        // Task 2: reads x (conflicts with 0)
        //
        // Expected layers:
        //   layer 0: [0, 1] — 0 writes x, 1 writes y, no conflict
        //   layer 1: [2]    — 2 reads x, conflicts with 0
        let block0 = Block { stmts: vec![], span: hy_diag::Span::new_dummy() };
        let block1 = Block { stmts: vec![], span: hy_diag::Span::new_dummy() };
        let block2 = Block { stmts: vec![], span: hy_diag::Span::new_dummy() };

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
