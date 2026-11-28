//! channel_liveness.rs — SPSC linear channel capability & nursery lifecycle invariants (Milestone 1, #4200).
//!
//! Enforces:
//! - Capability split: SPSC linear channel capabilities (`hy.send_cap[T]`, `hy.recv_cap[T]`) via `hy.pipe[T]()`.
//! - Affine transfer semantics: `send_cap` and `recv_cap` move strictly into spawned task closures (affine use-after-move: `HYB-E3001`).
//!   Reference capture (`&tx`, `&rx` or bare capture across task boundary) is rejected with `HYB-E3002`.
//! - Nursery exit terminal invariants: forward CFG dataflow analysis verifies all `send_cap` handles reach `Closed` (`HYB-E4004`),
//!   and all `recv_cap` handles reach `Drained` (`HYB-E4005`).

use hy_diag::{Diagnostic, Span};
use hy_syntax::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelTypestate {
    Fresh,
    Active,
    Closed,
    Drained,
    Moved,
}

#[derive(Debug, Clone)]
pub struct PipeInfo {
    pub tx: String,
    pub rx: String,
    pub tx_state: ChannelTypestate,
    pub rx_state: ChannelTypestate,
    pub def_span: Span,
}

#[derive(Debug, Clone, Default)]
pub struct ScopeState {
    pub pipes: Vec<PipeInfo>,
    pub moved_caps: HashSet<String>,
}

pub struct ChannelLivenessChecker {
    file_path: String,
    diags: Vec<Diagnostic>,
    scope_stack: Vec<ScopeState>,
    cap_to_pipe: HashMap<String, usize>,
    pipes_all: Vec<PipeInfo>,
}

impl ChannelLivenessChecker {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
            diags: Vec::new(),
            scope_stack: vec![ScopeState::default()],
            cap_to_pipe: HashMap::new(),
            pipes_all: Vec::new(),
        }
    }

    pub fn check_file(mut self, file: &File) -> Vec<Diagnostic> {
        for decl in &file.decls {
            if let Decl::Fn(f) = decl {
                self.check_fn(f);
            }
        }
        self.diags
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(ScopeState::default());
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.scope_stack.pop() {
            self.check_terminal_invariants(&scope);
            if let Some(parent) = self.scope_stack.last_mut() {
                parent.moved_caps.extend(scope.moved_caps);
            }
        }
    }

    fn register_pipe(&mut self, tx_name: String, rx_name: String, def_span: Span) {
        let pipe = PipeInfo {
            tx: tx_name.clone(),
            rx: rx_name.clone(),
            tx_state: ChannelTypestate::Fresh,
            rx_state: ChannelTypestate::Fresh,
            def_span,
        };

        let p_idx = self.pipes_all.len();
        self.pipes_all.push(pipe.clone());
        self.cap_to_pipe.insert(tx_name, p_idx);
        self.cap_to_pipe.insert(rx_name, p_idx);

        if let Some(cur) = self.scope_stack.last_mut() {
            cur.pipes.push(pipe);
        }
    }

    fn check_terminal_invariants(&mut self, scope: &ScopeState) {
        for pipe in &scope.pipes {
            // Check send capability terminal state
            if !scope.moved_caps.contains(&pipe.tx) && pipe.tx_state != ChannelTypestate::Closed {
                self.diags.push(Diagnostic::error(
                    "HYB-E4004",
                    &self.file_path,
                    pipe.def_span,
                    format!(
                        "linear send capability '{}' was not closed before scope exit",
                        pipe.tx
                    ),
                ));
            }

            // Check receive capability terminal state
            // If rx was moved into a subtask, the subtask assumed lifecycle responsibility
            if !scope.moved_caps.contains(&pipe.rx) && pipe.rx_state != ChannelTypestate::Drained {
                self.diags.push(Diagnostic::error(
                    "HYB-E4005",
                    &self.file_path,
                    pipe.def_span,
                    format!(
                        "linear receive capability '{}' was not drained before scope exit",
                        pipe.rx
                    ),
                ));
            }
        }
    }

    fn check_fn(&mut self, f: &FnDecl) {
        self.push_scope();
        if let Some(body) = &f.body {
            for stmt in &body.stmts {
                self.check_stmt(stmt);
            }
        }
        self.pop_scope();
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Local(decl) => {
                if decl.names.len() == 2 && decl.values.len() == 1 {
                    if self.is_pipe_call(&decl.values[0]) {
                        self.register_pipe(decl.names[0].clone(), decl.names[1].clone(), decl.span);
                        return;
                    }
                }
                for v in &decl.values {
                    self.check_expr(v);
                }
            }
            Stmt::Assign { targets, values, span, .. } => {
                if targets.len() == 2 && values.len() == 1 && self.is_pipe_call(&values[0]) {
                    if let (PlaceKind::Ident(tx_name), PlaceKind::Ident(rx_name)) = (&targets[0].kind, &targets[1].kind) {
                        self.register_pipe(tx_name.clone(), rx_name.clone(), *span);
                        return;
                    }
                }
                for v in values {
                    self.check_expr(v);
                }
            }
            Stmt::Expr { expr, .. } => {
                self.check_expr(expr);
            }
            Stmt::Task { callee, args, span } => {
                self.check_expr(callee);
                for arg in args {
                    self.check_task_arg(&arg.value, *span);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Iterate { expr, .. } => {
                        if let Some(cap) = self.get_cap_name(expr) {
                            self.mark_rx_drained(&cap);
                        }
                        self.check_expr(expr);
                    }
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr(lo);
                        self.check_expr(hi);
                    }
                    ForKind::While { cond } => {
                        self.check_expr(cond);
                    }
                    ForKind::Infinite => {}
                }
                self.push_scope();
                for s in &body.stmts {
                    self.check_stmt(s);
                }
                // The body is walked a second time so that a capability moved
                // inside a loop that can run more than once is reported as
                // moved twice (#4289 — `for i in 0..10 { task worker(move tx) }`
                // really does move `tx` on every iteration).
                //
                // That second walk must only happen when the loop can
                // actually reach a second iteration. When it cannot, the
                // second walk re-visits a move that happened exactly once and
                // reported it as "affine capability 'X' moved more than once"
                // (#4769). Two shapes make a second iteration impossible:
                //
                //  - the body ends in an unconditional `break`/`return`, so
                //    the first iteration leaves the loop
                //    (test_issue_4289_loop_unconditional_break_accept);
                //  - the range bounds make the trip count at most one
                //    (`for i in 0..1`), so there is no second iteration to
                //    reach regardless of how the body ends (#4769).
                //
                // When the trip count is unknown (a non-literal bound), the
                // conservative second walk is kept: an unknown trip count
                // may still iterate more than once.
                let exits_after_first = matches!(
                    body.stmts.last(),
                    Some(Stmt::Break { .. } | Stmt::Return { .. })
                );
                let single_iteration = self.is_single_iteration_loop_for_liveness(kind, body);
                if !exits_after_first && !single_iteration {
                    for s in &body.stmts {
                        self.check_stmt(s);
                    }
                }
                self.pop_scope();
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.check_stmt(i);
                }
                self.check_expr(cond);
                self.push_scope();
                for s in &then.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
                if let Some(eb) = otherwise {
                    self.push_scope();
                    self.check_stmt(eb);
                    self.pop_scope();
                }
            }
            Stmt::Block { block, .. } => {
                self.push_scope();
                for s in &block.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr(subject);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard);
                    }
                    self.push_scope();
                    for s in &arm.body.stmts {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                }
            }
            Stmt::Defer { call, .. } => {
                self.check_expr(call);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr(v);
                }
            }
            Stmt::Managed { body, .. } => {
                self.push_scope();
                for s in &body.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
            }
            Stmt::Send {
                chan, value, span, ..
            } => {
                if let Some(cap) = self.get_cap_name(chan) {
                    if self.is_moved(&cap) {
                        self.diags.push(Diagnostic::error(
                            "HYB-E3001",
                            &self.file_path,
                            *span,
                            format!("use of moved linear capability '{}'", cap),
                        ));
                    }
                }
                self.check_expr(value);
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Recv { chan, .. } => {
                            if let Some(cap) = self.get_cap_name(chan) {
                                if self.is_moved(&cap) {
                                    self.diags.push(Diagnostic::error(
                                        "HYB-E3001",
                                        &self.file_path,
                                        arm.body.span,
                                        format!("use of moved linear capability '{}'", cap),
                                    ));
                                } else {
                                    self.mark_rx_active(&cap);
                                }
                            }
                        }
                        SelectArmKind::Send { chan, value, .. } => {
                            if let Some(cap) = self.get_cap_name(chan) {
                                if self.is_moved(&cap) {
                                    self.diags.push(Diagnostic::error(
                                        "HYB-E3001",
                                        &self.file_path,
                                        arm.body.span,
                                        format!("use of moved linear capability '{}'", cap),
                                    ));
                                }
                            }
                            self.check_expr(value);
                        }
                        SelectArmKind::Default => {}
                    }
                    self.push_scope();
                    for s in &arm.body.stmts {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                }
            }
            _ => {}
        }
    }

    fn check_task_arg(&mut self, arg: &Expr, _call_span: Span) {
        match arg {
            Expr::Move { operand, span } => {
                if let Some(cap) = self.get_cap_name(operand) {
                    if self.is_moved(&cap) {
                        self.diags.push(Diagnostic::error(
                            "HYB-E3001",
                            &self.file_path,
                            *span,
                            format!("affine capability '{}' moved more than once", cap),
                        ));
                    } else {
                        self.mark_moved(&cap);
                    }
                }
            }
            Expr::AddrOf { operand, span } => {
                if let Some(cap) = self.get_cap_name(operand) {
                    self.diags.push(Diagnostic::error(
                        "HYB-E3002",
                        &self.file_path,
                        *span,
                        format!(
                            "reference capture '&{}' of linear capability across task boundary is forbidden",
                            cap
                        ),
                    ));
                }
            }
            Expr::Unary { op, operand, span } if matches!(op, UnOp::Addr) => {
                if let Some(cap) = self.get_cap_name(operand) {
                    self.diags.push(Diagnostic::error(
                        "HYB-E3002",
                        &self.file_path,
                        *span,
                        format!(
                            "reference capture '&{}' of linear capability across task boundary is forbidden",
                            cap
                        ),
                    ));
                }
            }
            Expr::Ident { name, span } => {
                if self.is_cap(name) {
                    self.diags.push(Diagnostic::error(
                        "HYB-E3002",
                        &self.file_path,
                        *span,
                        format!(
                            "unmoved capture '{}' across task boundary is forbidden; must be explicitly transferred with 'move'",
                            name
                        ),
                    ));
                }
            }
            _ => {
                self.check_expr(arg);
            }
        }
    }

    /// True when this loop's body provably runs at most once, so a second
    /// walk over it cannot observe a second occurrence of anything.
    ///
    /// Only a `Range` loop with literal bounds can be answered. When the
    /// bound is not a literal the answer is `false` — an unknown trip count
    /// keeps the conservative second walk, so a multi-iteration loop still
    /// gets its capability double-move reported (#4289).
    ///
    /// The trip count mirrors the loop semantics used elsewhere in the
    /// checker: `lo..hi` is exclusive and `lo..=hi` is inclusive, and a
    /// range whose upper bound is below its lower bound runs zero times.
    fn is_single_iteration_loop_for_liveness(
        &self,
        kind: &ForKind,
        _body: &Block,
    ) -> bool {
        let ForKind::Range { lo, hi, inclusive, .. } = kind else {
            return false;
        };
        let (Some(lo), Some(hi)) = (Self::as_int_lit_for_liveness(lo), Self::as_int_lit_for_liveness(hi)) else {
            return false;
        };
        let count = if *inclusive {
            if hi >= lo {
                hi.checked_sub(lo).and_then(|d| d.checked_add(1))
            } else {
                Some(0)
            }
        } else if hi > lo {
            hi.checked_sub(lo)
        } else {
            Some(0)
        };
        matches!(count, Some(c) if c <= 1)
    }

    /// Integer literal value, unwrapping parentheses and a leading minus.
    fn as_int_lit_for_liveness(expr: &Expr) -> Option<i64> {
        match expr {
            Expr::Lit {
                kind: LitKind::Int(s),
                ..
            } => crate::arith::int_lit_value(s).and_then(|v| i64::try_from(v).ok()),
            Expr::Paren { inner, .. } => Self::as_int_lit_for_liveness(inner),
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => Self::as_int_lit_for_liveness(operand).and_then(|v| v.checked_neg()),
            _ => None,
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Move { operand, span } => {
                if let Some(cap) = self.get_cap_name(operand) {
                    if self.is_moved(&cap) {
                        self.diags.push(Diagnostic::error(
                            "HYB-E3001",
                            &self.file_path,
                            *span,
                            format!("affine capability '{}' moved more than once", cap),
                        ));
                    } else {
                        self.mark_moved(&cap);
                    }
                }
            }
            Expr::Try { operand, .. } => {
                self.check_expr(operand);
            }
            Expr::FnLit { body, .. } => {
                self.push_scope();
                for s in &body.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
            }
            Expr::Call { callee, args, span } => {
                if let Expr::Field { base, name, .. } = callee.as_ref() {
                    if let Some(cap) = self.get_cap_name(base) {
                        if self.is_moved(&cap) {
                            self.diags.push(Diagnostic::error(
                                "HYB-E3001",
                                &self.file_path,
                                *span,
                                format!("use of moved linear capability '{}'", cap),
                            ));
                            return;
                        }

                        match name.as_str() {
                            "close" => {
                                self.mark_tx_closed(&cap);
                            }
                            "drain" => {
                                self.mark_rx_drained(&cap);
                            }
                            "send" => {
                                self.mark_tx_active(&cap);
                            }
                            "recv" => {
                                self.mark_rx_active(&cap);
                            }
                            _ => {}
                        }
                    }
                }
                self.check_expr(callee);
                for a in args {
                    self.check_expr(&a.value);
                }
            }
            Expr::FnLit { body, .. } => {
                self.push_scope();
                for s in &body.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
            }
            Expr::Receive { chan, span } => {
                if let Some(cap) = self.get_cap_name(chan) {
                    if self.is_moved(&cap) {
                        self.diags.push(Diagnostic::error(
                            "HYB-E3001",
                            &self.file_path,
                            *span,
                            format!("use of moved linear capability '{}'", cap),
                        ));
                    } else {
                        self.mark_rx_active(&cap);
                    }
                }
            }
            Expr::Ident { name, span } => {
                if self.is_moved(name) {
                    self.diags.push(Diagnostic::error(
                        "HYB-E3001",
                        &self.file_path,
                        *span,
                        format!("use of moved linear capability '{}'", name),
                    ));
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr(lhs);
                self.check_expr(rhs);
            }
            Expr::Unary { operand, .. } => {
                self.check_expr(operand);
            }
            Expr::Paren { inner, .. } => {
                self.check_expr(inner);
            }
            Expr::Index { base, index, .. } => {
                self.check_expr(base);
                self.check_expr(index);
            }
            _ => {}
        }
    }

    fn is_pipe_call(&self, expr: &Expr) -> bool {
        let mut e = expr;
        while match e {
            Expr::Paren { inner, .. } => {
                e = inner.as_ref();
                true
            }
            Expr::Try { operand, .. } => {
                e = operand.as_ref();
                true
            }
            _ => false,
        } {}
        if let Expr::Call { callee, .. } = e {
            let mut base = callee.as_ref();
            while let Expr::Index { base: inner, .. } | Expr::Paren { inner, .. } = base {
                base = inner.as_ref();
            }
            if let Expr::Field {
                base: pkg, name, ..
            } = base
            {
                if let Expr::Ident { name: pkg_name, .. } = pkg.as_ref() {
                    let head = name.split('[').next().unwrap_or(name.as_str());
                    if pkg_name == "hy" && (head == "pipe" || head == "new_pipe") {
                        return true;
                    }
                }
            }
            if let Expr::Ident { name, .. } = base {
                let head = name.split('[').next().unwrap_or(name.as_str());
                if head == "pipe" || head == "new_pipe" {
                    return true;
                }
            }
        }
        false
    }

    fn get_cap_name(&self, expr: &Expr) -> Option<String> {
        let mut e = expr;
        while let Expr::Paren { inner, .. } = e {
            e = inner.as_ref();
        }
        if let Expr::Ident { name, .. } = e {
            if self.is_cap(name) {
                return Some(name.clone());
            }
        }
        None
    }

    fn is_cap(&self, name: &str) -> bool {
        self.cap_to_pipe.contains_key(name)
    }

    fn is_moved(&self, name: &str) -> bool {
        for scope in self.scope_stack.iter().rev() {
            if scope.moved_caps.contains(name) {
                return true;
            }
        }
        false
    }

    fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.moved_caps.insert(name.to_string());
        }
    }

    fn mark_tx_closed(&mut self, name: &str) {
        for scope in self.scope_stack.iter_mut().rev() {
            for p in &mut scope.pipes {
                if p.tx == name {
                    p.tx_state = ChannelTypestate::Closed;
                    return;
                }
            }
        }
    }

    fn mark_tx_active(&mut self, name: &str) {
        for scope in self.scope_stack.iter_mut().rev() {
            for p in &mut scope.pipes {
                if p.tx == name && p.tx_state == ChannelTypestate::Fresh {
                    p.tx_state = ChannelTypestate::Active;
                    return;
                }
            }
        }
    }

    fn mark_rx_drained(&mut self, name: &str) {
        for scope in self.scope_stack.iter_mut().rev() {
            for p in &mut scope.pipes {
                if p.rx == name {
                    p.rx_state = ChannelTypestate::Drained;
                    return;
                }
            }
        }
    }

    fn mark_rx_active(&mut self, name: &str) {
        for scope in self.scope_stack.iter_mut().rev() {
            for p in &mut scope.pipes {
                if p.rx == name && p.rx_state == ChannelTypestate::Fresh {
                    p.rx_state = ChannelTypestate::Active;
                    return;
                }
            }
        }
    }
}
