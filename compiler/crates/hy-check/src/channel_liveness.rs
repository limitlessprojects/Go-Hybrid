//! channel_liveness.rs 鈥?SPSC linear channel capability & nursery lifecycle invariants (Milestone 1, #4200).
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
    /// Caps moved on EVERY path reaching this point 鈥?excuses the close /
    /// drain obligation at `check_terminal_invariants` (#4322: conditional
    /// merge must intersect, not union).
    pub moved_caps: HashSet<String>,
    /// Caps moved on AT LEAST ONE path 鈥?`is_moved` consults this so a
    /// one-branch `task w(move tx)` still flags a later `tx.send` as
    /// use-after-move on the taken path.
    pub maybe_caps: HashSet<String>,
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
                parent.maybe_caps.extend(scope.maybe_caps);
            }
        }
    }

    /// Mutable checker state a conditional arm can touch (#4322 fix):
    /// scope stacks carry both pipe typestates and `moved_caps`;
    /// `cap_to_pipe`/`pipes_all` grow on arm-local `hy.pipe` registrations
    /// and must not leak to sibling arms or to the join path.
    fn branch_snapshot(&self) -> (Vec<ScopeState>, HashMap<String, usize>, Vec<PipeInfo>) {
        (
            self.scope_stack.clone(),
            self.cap_to_pipe.clone(),
            self.pipes_all.clone(),
        )
    }

    fn restore_branch(&mut self, snap: &(Vec<ScopeState>, HashMap<String, usize>, Vec<PipeInfo>)) {
        self.scope_stack = snap.0.clone();
        self.cap_to_pipe = snap.1.clone();
        self.pipes_all = snap.2.clone();
    }

    /// Path-sensitive meet over per-arm end states (fixes #4322).
    /// `stacks` holds one entry per arm, plus one snapshot entry for an
    /// implicit untouched path (missing `else`, guarded `match` arms).
    ///
    /// Meet semantics per cap:
    /// - `maybe_caps` = union 鈥?a move on ANY arm must still flag a later
    ///   use (use-after-move exists on that path);
    /// - `moved_caps` = intersection 鈥?the close/drain obligation is excused
    ///   only when EVERY path transferred the cap;
    /// - pipe typestate = the least-progressed arm value, UNLESS every arm
    ///   discharged the obligation 鈥?then the merged cap is `Closed` (all
    ///   arms closed it) or recorded as moved (mixed close/move discharge
    ///   鈥?any post-join use is use-after-move anyway).
    fn apply_branch_meet(
        &mut self,
        snap: &(Vec<ScopeState>, HashMap<String, usize>, Vec<PipeInfo>),
        stacks: Vec<Vec<ScopeState>>,
    ) {
        let arm_moved = |stacks: &[Vec<ScopeState>], k: usize, name: &str| {
            stacks[k]
                .iter()
                .any(|s| s.moved_caps.contains(name) || s.maybe_caps.contains(name))
        };
        let mut merged = snap.0.clone();
        for (si, scope) in merged.iter_mut().enumerate() {
            let mut all_names: HashSet<String> = HashSet::new();
            for s in &stacks {
                for n in s[si].moved_caps.iter().chain(s[si].maybe_caps.iter()) {
                    all_names.insert(n.clone());
                }
            }
            // Definite moves: names moved on every arm (intersection).
            let mut moved: HashSet<String> = all_names
                .iter()
                .filter(|n| (0..stacks.len()).all(|k| arm_moved(&stacks, k, n)))
                .cloned()
                .collect();
            // Maybe moves: union.
            let maybe: HashSet<String> = all_names
                .iter()
                .filter(|n| (0..stacks.len()).any(|k| arm_moved(&stacks, k, n)))
                .cloned()
                .collect();
            for p in scope.pipes.iter_mut() {
                let tx_all_closed = stacks
                    .iter()
                    .enumerate()
                    .all(|(k, s)| s[si].pipes.iter().any(|q| q.tx == p.tx && q.tx_state == ChannelTypestate::Closed) || arm_moved(&stacks, k, &p.tx));
                let rx_all_drained = stacks
                    .iter()
                    .enumerate()
                    .all(|(k, s)| s[si].pipes.iter().any(|q| q.rx == p.rx && q.rx_state == ChannelTypestate::Drained) || arm_moved(&stacks, k, &p.rx));
                if tx_all_closed {
                    if stacks.iter().all(|s| {
                        s[si].pipes
                            .iter()
                            .any(|q| q.tx == p.tx && q.tx_state == ChannelTypestate::Closed)
                    }) {
                        p.tx_state = ChannelTypestate::Closed;
                    } else {
                        moved.insert(p.tx.clone());
                    }
                } else {
                    for s in &stacks {
                        if let Some(q) = s[si].pipes.iter().find(|q| q.tx == p.tx) {
                            p.tx_state = meet_state(p.tx_state, q.tx_state);
                        }
                    }
                }
                if rx_all_drained {
                    if stacks.iter().all(|s| {
                        s[si].pipes
                            .iter()
                            .any(|q| q.rx == p.rx && q.rx_state == ChannelTypestate::Drained)
                    }) {
                        p.rx_state = ChannelTypestate::Drained;
                    } else {
                        moved.insert(p.rx.clone());
                    }
                } else {
                    for s in &stacks {
                        if let Some(q) = s[si].pipes.iter().find(|q| q.rx == p.rx) {
                            p.rx_state = meet_state(p.rx_state, q.rx_state);
                        }
                    }
                }
            }
            scope.moved_caps = moved;
            scope.maybe_caps = maybe;
        }
        self.scope_stack = merged;
        self.cap_to_pipe = snap.1.clone();
        self.pipes_all = snap.2.clone();
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
                let has_unconditional_exit = matches!(
                    body.stmts.last(),
                    Some(Stmt::Break { .. } | Stmt::Return { .. })
                );
                if !has_unconditional_exit {
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
                // Literal conditions take exactly one path 鈥?run that arm
                // unconditionally (no meet, no fallthrough arm).
                match lit_bool(cond) {
                    Some(true) => {
                        self.push_scope();
                        for s in &then.stmts {
                            self.check_stmt(s);
                        }
                        self.pop_scope();
                    }
                    Some(false) => {
                        if let Some(eb) = otherwise {
                            self.push_scope();
                            self.check_stmt(eb);
                            self.pop_scope();
                        }
                    }
                    None => {
                        let snap = self.branch_snapshot();
                        let mut stacks = Vec::with_capacity(2);
                        self.restore_branch(&snap);
                        self.push_scope();
                        for s in &then.stmts {
                            self.check_stmt(s);
                        }
                        self.pop_scope();
                        stacks.push(self.scope_stack.clone());
                        if let Some(eb) = otherwise {
                            self.restore_branch(&snap);
                            self.push_scope();
                            self.check_stmt(eb);
                            self.pop_scope();
                            stacks.push(self.scope_stack.clone());
                        } else {
                            // No else: the untouched path is an arm of its own.
                            stacks.push(snap.0.clone());
                        }
                        self.apply_branch_meet(&snap, stacks);
                    }
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
                // A path can escape the match only through a guard (`x if
                // cond` whose cond is false) or a missing arm 鈥?E5001 owns
                // exhaustiveness, so a fallthrough arm is needed only when
                // some arm is guarded and no unguarded `_` covers the rest.
                let needs_fallthrough = arms.is_empty()
                    || (arms.iter().any(|a| a.guard.is_some())
                        && !arms
                            .iter()
                            .any(|a| a.guard.is_none() && matches!(a.pattern, MatchPattern::Wildcard)));
                let snap = self.branch_snapshot();
                let mut stacks = Vec::with_capacity(arms.len() + 1);
                for arm in arms {
                    self.restore_branch(&snap);
                    if let Some(guard) = &arm.guard {
                        self.check_expr(guard);
                    }
                    self.push_scope();
                    for s in &arm.body.stmts {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                    stacks.push(self.scope_stack.clone());
                }
                if needs_fallthrough {
                    stacks.push(snap.0.clone());
                }
                self.apply_branch_meet(&snap, stacks);
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
                // Each arm runs from the same pre-select state; the merged
                // state meets them. A select without `default` blocks
                // forever on the uncovered path 鈥?a path that never reaches
                // scope exit carries no terminal obligation, so no
                // fallthrough arm is needed.
                let snap = self.branch_snapshot();
                let mut stacks = Vec::with_capacity(arms.len());
                for arm in arms {
                    self.restore_branch(&snap);
                    self.push_scope();
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
                    for s in &arm.body.stmts {
                        self.check_stmt(s);
                    }
                    self.pop_scope();
                    stacks.push(self.scope_stack.clone());
                }
                self.apply_branch_meet(&snap, stacks);
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
            if scope.maybe_caps.contains(name) {
                return true;
            }
        }
        false
    }

    fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.moved_caps.insert(name.to_string());
            scope.maybe_caps.insert(name.to_string());
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

/// Least-progressed of two typestates along Fresh < Active < terminal
/// (Closed/Drained/Moved). Used by `apply_branch_meet`: after a branch the
/// merged state may claim only the progress EVERY arm made.
fn meet_state(a: ChannelTypestate, b: ChannelTypestate) -> ChannelTypestate {
    fn rank(s: ChannelTypestate) -> u8 {
        match s {
            ChannelTypestate::Fresh => 0,
            ChannelTypestate::Active => 1,
            ChannelTypestate::Closed | ChannelTypestate::Drained | ChannelTypestate::Moved => 2,
        }
    }
    if rank(a) <= rank(b) {
        a
    } else {
        b
    }
}

/// A `Lit bool` condition's taken path (`if true`/`if false`), parens
/// stripped; `None` for non-literal conditions.
fn lit_bool(e: &Expr) -> Option<bool> {
    let mut e = e;
    while let Expr::Paren { inner, .. } = e {
        e = inner.as_ref();
    }
    if let Expr::Lit {
        kind: LitKind::Bool(b),
        ..
    } = e
    {
        Some(*b)
    } else {
        None
    }
}
