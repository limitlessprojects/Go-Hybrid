//! auto_schedule.rs — automatic fork-join scheduling emission (issue #1962).
//!
//! This is the emitter-side counterpart to hy-check's effect analysis.
//! It collects inline task blocks inside an `auto { ... }` scope, analyzes
//! their read/write effects, schedules them into parallel layers, and
//! generates Go code with sync.WaitGroup for each layer.

use std::collections::HashSet;

use hy_syntax::{Block, Expr, InterpPart, Stmt};

/// The read/write effect summary of a single task.
#[derive(Debug, Clone, Default)]
pub(crate) struct TaskEffect {
    pub reads: HashSet<String>,
    pub writes: HashSet<String>,
}

impl TaskEffect {
    pub fn read(&mut self, name: &str) {
        self.reads.insert(name.to_string());
    }

    pub fn write(&mut self, name: &str) {
        self.writes.insert(name.to_string());
    }

    pub fn conflicts_with(&self, other: &TaskEffect) -> bool {
        if !self.writes.is_disjoint(&other.writes) {
            return true;
        }
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

fn analyze_stmt(stmt: &Stmt, effect: &mut TaskEffect) {
    match stmt {
        Stmt::Local(local) => {
            for v in &local.values {
                analyze_expr(v, effect);
            }
            for name in &local.names {
                effect.write(name);
            }
        }
        Stmt::Assign { targets, values, .. } => {
            for v in values {
                analyze_expr(v, effect);
            }
            for t in targets {
                effect.write(t.root());
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
            analyze_block_stmts(&then.stmts, effect);
            if let Some(otherwise) = otherwise {
                analyze_stmt(otherwise, effect);
            }
        }
        Stmt::For { body, .. } => {
            analyze_block_stmts(&body.stmts, effect);
        }
        Stmt::Block { block, .. } => {
            analyze_block_stmts(&block.stmts, effect);
        }
        Stmt::TaskBlock { body, .. } => {
            analyze_block_stmts(&body.stmts, effect);
        }
        Stmt::Auto { body, .. } => {
            analyze_block_stmts(&body.stmts, effect);
        }
        Stmt::Managed { body, .. } => {
            analyze_block_stmts(&body.stmts, effect);
        }
        _ => {}
    }
}

fn analyze_block_stmts(stmts: &[Stmt], effect: &mut TaskEffect) {
    for stmt in stmts {
        analyze_stmt(stmt, effect);
    }
}

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
        Expr::Unary { operand, .. } => {
            analyze_expr(operand, effect);
        }
        Expr::Binary { lhs, rhs, .. } => {
            analyze_expr(lhs, effect);
            analyze_expr(rhs, effect);
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
        Expr::Field { base, .. } => {
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
        _ => {}
    }
}

/// Result of scheduling an auto block.
pub(crate) struct ScheduleResult<'a> {
    /// Layers of task indices (each layer runs in parallel).
    pub layers: Vec<Vec<usize>>,
    /// The collected task blocks (in source order).
    pub tasks: Vec<&'a Block>,
}

/// Collect all inline task blocks from an auto block and schedule them.
pub(crate) fn schedule_block(block: &Block) -> ScheduleResult<'_> {
    let mut task_blocks: Vec<&Block> = Vec::new();

    for stmt in &block.stmts {
        if let Stmt::TaskBlock { body, .. } = stmt {
            task_blocks.push(body);
        }
    }

    let effects: Vec<TaskEffect> = task_blocks.iter().map(|b| analyze_block(b)).collect();

    let mut layers: Vec<Vec<usize>> = Vec::new();

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

    ScheduleResult { layers, tasks: task_blocks }
}
