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
use std::collections::HashSet;

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
    pub shadowed_vars: HashSet<String>,
}

pub struct ChannelLivenessChecker {
    file_path: String,
    diags: Vec<Diagnostic>,
    scope_stack: Vec<ScopeState>,
}

impl ChannelLivenessChecker {
    pub fn new(file_path: String) -> Self {
        Self {
            file_path,
            diags: Vec::new(),
            scope_stack: vec![ScopeState::default()],
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
        if let Some(scope) = self.scope_stack.last_mut() {
            for p in &f.params {
                scope.shadowed_vars.insert(p.name.clone());
            }
        }
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
                for v in &decl.values {
                    self.check_expr(v);
                }
                if decl.names.len() == 2 && decl.values.len() == 1 && self.is_pipe_call(&decl.values[0]) {
                    let tx_name = decl.names[0].clone();
                    let rx_name = decl.names[1].clone();

                    let pipe = PipeInfo {
                        tx: tx_name.clone(),
                        rx: rx_name.clone(),
                        tx_state: ChannelTypestate::Fresh,
                        rx_state: ChannelTypestate::Fresh,
                        def_span: decl.span,
                    };

                    if let Some(cur) = self.scope_stack.last_mut() {
                        cur.shadowed_vars.remove(&tx_name);
                        cur.shadowed_vars.remove(&rx_name);
                        cur.pipes.push(pipe);
                    }
                } else {
                    if let Some(cur) = self.scope_stack.last_mut() {
                        for name in &decl.names {
                            cur.shadowed_vars.insert(name.clone());
                        }
                    }
                }
            }
            Stmt::Assign { values, .. } => {
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
                if let Some(scope) = self.scope_stack.last_mut() {
                    match kind {
                        ForKind::Range { var, .. } => {
                            scope.shadowed_vars.insert(var.clone());
                        }
                        ForKind::Iterate { vars, .. } => {
                            for v in vars {
                                scope.shadowed_vars.insert(v.clone());
                            }
                        }
                        _ => {}
                    }
                }
                for s in &body.stmts {
                    self.check_stmt(s);
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
                    self.check_stmt(eb);
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
                                self.mark_rx_drained(&cap);
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
            Expr::FnLit { params, body, .. } => {
                self.push_scope();
                if let Some(scope) = self.scope_stack.last_mut() {
                    for p in params {
                        scope.shadowed_vars.insert(p.name.clone());
                    }
                }
                for s in &body.stmts {
                    self.check_stmt(s);
                }
                self.pop_scope();
            }
            Expr::AddrOf { operand, .. } => {
                self.check_expr(operand);
            }

            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr(base);
                if let Some(lo) = lo {
                    self.check_expr(lo);
                }
                if let Some(hi) = hi {
                    self.check_expr(hi);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr(&f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr(it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr(k);
                    self.check_expr(v);
                }
            }
            Expr::Interp { parts, .. } => {
                for part in parts {
                    if let InterpPart::Expr(e) = part {
                        self.check_expr(e);
                    }
                }
            }
            _ => {}
        }
    }

    fn is_pipe_call(&self, expr: &Expr) -> bool {
        let mut e = expr;
        while let Expr::Paren { inner, .. } = e {
            e = inner.as_ref();
        }
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
        for scope in self.scope_stack.iter().rev() {
            if scope.shadowed_vars.contains(name) {
                return false;
            }
            for p in &scope.pipes {
                if p.tx == name || p.rx == name {
                    return true;
                }
            }
        }
        false
    }

    fn is_moved(&self, name: &str) -> bool {
        for scope in self.scope_stack.iter().rev() {
            if scope.shadowed_vars.contains(name) {
                return false;
            }
            if scope.moved_caps.contains(name) {
                return true;
            }
            for p in &scope.pipes {
                if p.tx == name || p.rx == name {
                    return false;
                }
            }
        }
        false
    }

    fn mark_moved(&mut self, name: &str) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.moved_caps.insert(name.to_string());
        }
        for scope in self.scope_stack.iter_mut().rev() {
            for p in &scope.pipes {
                if p.tx == name || p.rx == name {
                    scope.moved_caps.insert(name.to_string());
                    return;
                }
            }
        }
    }

    fn mark_tx_closed(&mut self, name: &str) {
        for scope in self.scope_stack.iter_mut().rev() {
            if scope.shadowed_vars.contains(name) {
                return;
            }
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
            if scope.shadowed_vars.contains(name) {
                return;
            }
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
            if scope.shadowed_vars.contains(name) {
                return;
            }
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
            if scope.shadowed_vars.contains(name) {
                return;
            }
            for p in &mut scope.pipes {
                if p.rx == name && p.rx_state == ChannelTypestate::Fresh {
                    p.rx_state = ChannelTypestate::Active;
                    return;
                }
            }
        }
    }
}
