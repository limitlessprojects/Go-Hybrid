//! walk.rs — the checker's traversal foundation.
//!
//! Every statement in a function body gets a monotonically increasing INDEX
//! (pre-order, source order). Two facts ride on those indices:
//!
//!   * `last_use[name]` — the max index of any reference to a binding (the
//!     non-lexical windows of A3 map-slot loans);
//!   * `loop_end` / `managed_end` — the last index inside a loop body /
//!     managed block ("the loan lives across the whole loop body" for A2;
//!     managed-block suppression ranges for O6).
//!
//! `own_uses` / `own_calls` collect the uses/calls of ONE statement only —
//! never recursing into child statements, which are separate flat entries
//! with their own indices. Closure (FnLit) bodies are skipped here — uses,
//! calls, loans and freeze stay outside (the A5 perimeter; closure captures
//! cannot be given Tier-0 loan windows). The closure BOUNDARY statements
//! (task/send/select-send) ARE checked — by the closures.rs pass, fail-closed
//! (O4.9); corpus 10's http.handler shape has no such statements and stays
//! ACCEPT under the scoped claim (Q1).

use crate::model::as_addr_of;
use hy_diag::Span;
use hy_syntax::{Block, CallArg, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Flat statement list + structural facts (Pass A)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub(crate) enum ScopeEvent<'a> {
    Push,
    Pop,
    SelectRecv {
        names: &'a [String],
        chan: &'a Expr,
    },
    MatchBind {
        name: &'a str,
        subject: &'a Expr,
    },
}

#[derive(Debug, Default)]
pub(crate) struct FlatInfo<'a> {
    /// every statement, pre-order, with its index
    pub stmts: Vec<(&'a Stmt, usize)>,
    /// for-loop index -> last index inside its body (== the loop index when
    /// the body is empty)
    pub loop_end: HashMap<usize, usize>,
    /// loop-statement index -> last index inside its body for LoopStmt (ForKind::Infinite).
    pub infinite_loop_end: HashMap<usize, usize>,
    /// managed-block index -> last index inside its body
    pub managed_end: HashMap<usize, usize>,
    /// binding name -> last statement index referencing it
    pub last_use: HashMap<String, usize>,
    /// Scope events keyed by statement index (issue #14): applied BEFORE
    /// the statement is processed, in order. Multiple events at the same
    /// index (e.g. when a nested construct and its enclosing `if`/`for`
    /// end at the same index) are preserved in sequence so every `Push`
    /// has a matching `Pop`.
    pub events: HashMap<usize, Vec<ScopeEvent<'a>>>,
    next: usize,
}

impl<'a> FlatInfo<'a> {
    fn mark(&mut self, idx: usize, push: bool, pop: bool) {
        let e = self.events.entry(idx).or_default();
        if pop {
            e.push(ScopeEvent::Pop);
        }
        if push {
            e.push(ScopeEvent::Push);
        }
    }

    fn mark_event(&mut self, idx: usize, ev: ScopeEvent<'a>) {
        self.events.entry(idx).or_default().push(ev);
    }

    /// Is statement `idx` lexically inside a `managed` block (O6.1)?
    pub(crate) fn in_managed(&self, idx: usize) -> bool {
        self.managed_end.iter().any(|(&k, &v)| k < idx && idx <= v)
    }

    /// Is statement `idx` lexically inside a `for`/`loop` body?
    pub(crate) fn in_loop(&self, idx: usize) -> bool {
        self.loop_end.iter().any(|(&k, &v)| k < idx && idx <= v)
    }

    /// Is statement `idx` lexically inside a `loop` statement (`ForKind::Infinite`,
    /// LX14 label scope)? `break loop` / `continue loop` outside any `loop` statement has no
    /// target — the checker rejects it (HYB-E3010).
    pub(crate) fn in_infinite_loop(&self, idx: usize) -> bool {
        self.infinite_loop_end.iter().any(|(&k, &v)| k < idx && idx <= v)
    }

    /// The loop-extent end for a for-statement at `for_idx`.
    pub(crate) fn loop_end_of(&self, for_idx: usize) -> usize {
        self.loop_end.get(&for_idx).copied().unwrap_or(for_idx)
    }
}

/// Flatten a function body: assign indices, record structural extents and
/// per-binding last-use indices.
pub(crate) fn flatten<'a>(body: &'a Block) -> FlatInfo<'a> {
    flatten_with_results(body, &[])
}

pub(crate) fn flatten_with_results<'a>(body: &'a Block, named_results: &[String]) -> FlatInfo<'a> {
    let mut out = FlatInfo::default();
    walk_block(&mut out, body, named_results);
    out
}

fn walk_block<'a>(out: &mut FlatInfo<'a>, block: &'a Block, named_results: &[String]) {
    for s in &block.stmts {
        walk_one(out, s, named_results);
    }
}

fn walk_one<'a>(out: &mut FlatInfo<'a>, stmt: &'a Stmt, named_results: &[String]) {
    if let Stmt::If {
        init: Some(i),
        then,
        otherwise,
        ..
    } = stmt
    {
        // ONE implicit scope for the if (init vars stay visible through
        // the whole construct, Go rule); `init` executes BEFORE `cond`,
        // so walk `init` first inside the if-scope and assign `Stmt::If`
        // (`cond`) the index immediately after `init`.
        out.mark(out.next, true, false);
        walk_one(out, i, named_results);

        let idx = out.next;
        out.next += 1;
        out.stmts.push((stmt, idx));

        let mut uses = Vec::new();
        own_uses_with_results(stmt, named_results, &mut uses);
        for u in uses {
            let e = out.last_use.entry(u.name).or_insert(idx);
            if *e < idx {
                *e = idx;
            }
        }

        scoped_block(out, then, named_results);
        if let Some(o) = otherwise {
            walk_one(out, o, named_results);
        }
        out.mark(out.next, false, true); // Pop at the next stmt (or trailing)
        return;
    }

    let idx = out.next;
    out.next += 1;
    out.stmts.push((stmt, idx));

    let mut uses = Vec::new();
    own_uses_with_results(stmt, named_results, &mut uses);
    for u in uses {
        let e = out.last_use.entry(u.name).or_insert(idx);
        if *e < idx {
            *e = idx;
        }
    }

    match stmt {
        Stmt::If {
            then,
            otherwise,
            ..
        } => {
            // ONE implicit scope for the if; the then/else bodies open their
            // own scopes below.
            out.mark(idx, true, false);
            scoped_block(out, then, named_results);
            if let Some(o) = otherwise {
                walk_one(out, o, named_results);
            }
            out.mark(out.next, false, true); // Pop at the next stmt (or trailing)
        }
        Stmt::For { kind, body, .. } => {
            // the loop's implicit scope (range/iterate vars) ...
            out.mark(idx, true, false);
            scoped_block(out, body, named_results); // ... and the body's own scope
            let end = out.next.saturating_sub(1);
            out.loop_end.insert(idx, end);
            if matches!(kind, hy_syntax::ForKind::Infinite) {
                out.infinite_loop_end.insert(idx, end);
            }
            out.mark(out.next, false, true);
        }
        Stmt::Managed { body, .. } => {
            scoped_block(out, body, named_results);
            out.managed_end.insert(idx, out.next.saturating_sub(1));
        }
        Stmt::Match { subject, arms, .. } => {
            for arm in arms {
                let first = out.next;
                let mut ran = false;
                for s in &arm.body.stmts {
                    if !ran {
                        out.mark_event(first, ScopeEvent::Push);
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(name),
                            ..
                        } = &arm.pattern
                        {
                            out.mark_event(
                                first,
                                ScopeEvent::MatchBind {
                                    name: name.as_str(),
                                    subject,
                                },
                            );
                        }
                        ran = true;
                    }
                    walk_one(out, s, named_results);
                }
                if ran {
                    out.mark_event(out.next, ScopeEvent::Pop);
                }
            }
        }
        Stmt::Block { block, .. } => scoped_block(out, block, named_results),
        Stmt::Select { arms, .. } => {
            for arm in arms {
                let first = out.next;
                let mut ran = false;
                for s in &arm.body.stmts {
                    if !ran {
                        out.mark_event(first, ScopeEvent::Push);
                        if let SelectArmKind::Recv { names, chan } = &arm.kind {
                            out.mark_event(first, ScopeEvent::SelectRecv { names, chan });
                        }
                        ran = true;
                    }
                    walk_one(out, s, named_results);
                }
                if ran {
                    out.mark_event(out.next, ScopeEvent::Pop);
                }
            }
        }
        _ => {}
    }
}

/// Walk a block's statements inside their OWN scope: Push on the first
/// statement, Pop on the index after the last one (or trailing).
fn scoped_block<'a>(out: &mut FlatInfo<'a>, block: &'a Block, named_results: &[String]) {
    let first = out.next;
    let mut ran = false;
    for s in &block.stmts {
        if !ran {
            out.mark(first, true, false);
            ran = true;
        }
        walk_one(out, s, named_results);
    }
    if ran {
        out.mark(out.next, false, true);
    }
}

// ---------------------------------------------------------------------------
// Uses (A1b/A7 use-after-move sites)
// ---------------------------------------------------------------------------

/// A reference to a binding: the root of a place or expression chain.
#[derive(Debug, Clone)]
pub(crate) struct Use {
    pub name: String,
    pub span: Span,
    /// true when the use is an assignment TARGET (write form).
    pub is_write: bool,
    /// true when the use is the direct operand of a `move x` at its own
    /// transfer site (exempt from the use check unless already moved).
    pub via_move: bool,
    /// true when the use is the direct operand of an address-of `&x`
    /// (T24: `&` on a map-slot loan binding is HYB-E3104).
    pub via_addr: bool,
}

/// Collect the uses of THIS statement only (child statements are separate
/// flat entries with their own indices).
pub(crate) fn own_uses(stmt: &Stmt, out: &mut Vec<Use>) {
    own_uses_with_results(stmt, &[], out);
}

pub(crate) fn own_uses_with_results(stmt: &Stmt, named_results: &[String], out: &mut Vec<Use>) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                expr_uses(v, out);
            }
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                place_uses(t, out);
            }
            for v in values {
                expr_uses(v, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_uses(expr, out),
        Stmt::Send { chan, value, .. } => {
            expr_uses(chan, out);
            expr_uses(value, out);
        }
        Stmt::If { cond, .. } => expr_uses(cond, out),
        Stmt::For { kind, .. } => match kind {
            ForKind::While { cond } => expr_uses(cond, out),
            ForKind::Range { lo, hi, .. } => {
                expr_uses(lo, out);
                expr_uses(hi, out);
            }
            ForKind::Iterate { expr, .. } => expr_uses(expr, out),
            ForKind::Infinite => {}
        },
        Stmt::Return { values, span } => {
            if values.is_empty() {
                for name in named_results {
                    out.push(Use {
                        name: name.clone(),
                        span: *span,
                        is_write: false,
                        via_move: false,
                        via_addr: false,
                    });
                }
            } else {
                for v in values {
                    expr_uses(v, out);
                }
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_uses(callee, out);
            for a in args {
                expr_uses(&a.value, out);
            }
        }
        Stmt::Managed { .. } => {}
        Stmt::Match { subject, arms, .. } => {
            expr_uses(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    let bound_name = match &arm.pattern {
                        hy_syntax::MatchPattern::Path {
                            binding: Some(n), ..
                        } => Some(n.as_str()),
                        _ => None,
                    };
                    let mut g_uses = Vec::new();
                    expr_uses(g, &mut g_uses);
                    for u in g_uses {
                        if Some(u.name.as_str()) != bound_name {
                            out.push(u);
                        }
                    }
                }
            }
        }
        Stmt::Defer { call, .. } => expr_uses(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Block { .. } => {}
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        expr_uses(chan, out);
                        expr_uses(value, out);
                    }
                    SelectArmKind::Recv { chan, .. } => expr_uses(chan, out),
                    SelectArmKind::Default => {}
                }
            }
        }
    }
}

fn place_uses(place: &Place, out: &mut Vec<Use>) {
    match &place.kind {
        PlaceKind::Ident(n) => out.push(Use {
            name: n.clone(),
            span: place.span,
            is_write: true,
            via_move: false,
            via_addr: false,
        }),
        PlaceKind::Index { base, index } => {
            place_uses(base, out);
            expr_uses(index, out);
        }
        PlaceKind::Field { base, .. } => place_uses(base, out),
        PlaceKind::AddrOf { base } => place_uses(base, out),
    }
}

pub(crate) fn expr_uses(expr: &Expr, out: &mut Vec<Use>) {
    match expr {
        Expr::Ident { name, span } => out.push(Use {
            name: name.clone(),
            span: *span,
            is_write: false,
            via_move: false,
            via_addr: false,
        }),
        Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    expr_uses(e, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_uses(lhs, out);
            expr_uses(rhs, out);
        }
        Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            span,
        }
        | Expr::AddrOf { operand, span } => match crate::model::unwrap_parens(operand) {
            Expr::Ident { name, .. } => out.push(Use {
                name: name.clone(),
                span: *span,
                is_write: false,
                via_move: false,
                via_addr: true,
            }),
            _ => expr_uses(operand, out),
        },
        Expr::Unary { operand, .. } => expr_uses(operand, out),
        Expr::Move { operand, span } => match crate::model::unwrap_parens(operand) {
            Expr::Ident { name, .. } => out.push(Use {
                name: name.clone(),
                span: *span,
                is_write: false,
                via_move: true,
                via_addr: false,
            }),
            Expr::Field { .. } => {
                if let Some(root) = crate::model::unwrap_parens(operand).root_ident() {
                    out.push(Use {
                        name: root.to_string(),
                        span: *span,
                        is_write: false,
                        via_move: true,
                        via_addr: false,
                    });
                } else {
                    expr_uses(operand, out);
                }
            }
            _ => expr_uses(operand, out),
        },
        Expr::Try { operand, .. } => expr_uses(operand, out),
        Expr::Receive { chan, .. } => expr_uses(chan, out),
        Expr::Index { base, index, .. } => {
            expr_uses(base, out);
            expr_uses(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_uses(base, out);
            if let Some(lo) = lo {
                expr_uses(lo, out);
            }
            if let Some(hi) = hi {
                expr_uses(hi, out);
            }
        }
        Expr::Field { base, .. } => expr_uses(base, out),
        Expr::Call { callee, args, .. } => {
            expr_uses(callee, out);
            for a in args {
                expr_uses(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_uses(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                expr_uses(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_uses(k, out);
                expr_uses(v, out);
            }
        }
        // A5 perimeter: closure bodies are not analyzed HERE (uses/calls/
        // loans stay outside); their boundary statements are checked by the
        // closures.rs pass (O4.9).
        Expr::FnLit { .. } => {}
        Expr::Paren { inner, .. } => expr_uses(inner, out),
    }
}

// ---------------------------------------------------------------------------
// Calls (A2 loan windows, A7 consumes, handle-argument loans)
// ---------------------------------------------------------------------------

/// One call site, borrowed from the AST.
#[derive(Debug, Clone, Copy)]
pub(crate) struct CallRef<'a> {
    pub callee: &'a Expr,
    pub args: &'a [CallArg],
    pub span: Span,
}

/// Collect the calls of THIS statement only. For-statement ITERATE
/// expressions are excluded here: their borrow calls get loop-extent loans
/// and are processed by the For handler (loans.rs / lib.rs dispatch).
pub(crate) fn own_calls<'a>(stmt: &'a Stmt, out: &mut Vec<CallRef<'a>>) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                expr_calls(v, out);
            }
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                if let PlaceKind::Index { index, .. } = &t.kind {
                    expr_calls(index, out);
                }
            }
            for v in values {
                expr_calls(v, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_calls(expr, out),
        Stmt::Send { chan, value, .. } => {
            expr_calls(chan, out);
            expr_calls(value, out);
        }
        Stmt::If { cond, .. } => expr_calls(cond, out),
        Stmt::For { kind, .. } => match kind {
            ForKind::While { cond } => expr_calls(cond, out),
            ForKind::Range { lo, hi, .. } => {
                expr_calls(lo, out);
                expr_calls(hi, out);
            }
            ForKind::Iterate { .. } => {} // handled with loop-extent loans
            ForKind::Infinite => {}
        },
        Stmt::Return { values, .. } => {
            for v in values {
                expr_calls(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_calls(callee, out);
            for a in args {
                expr_calls(&a.value, out);
            }
        }
        Stmt::Managed { .. } => {}
        Stmt::Match { subject, arms, .. } => {
            expr_calls(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    expr_calls(g, out);
                }
            }
        }
        Stmt::Defer { call, .. } => expr_calls(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Block { .. } => {}
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        expr_calls(chan, out);
                        expr_calls(value, out);
                    }
                    SelectArmKind::Recv { chan, .. } => expr_calls(chan, out),
                    SelectArmKind::Default => {}
                }
            }
        }
    }
}

/// Collect the `FnLit` expressions of THIS statement (#123). Closure BODIES
/// are not descended here — each closure is handled as a unit by the caller
/// (the A5 perimeter keeps flatten/uses/calls outside them).
pub(crate) fn own_fnlits<'a>(stmt: &'a Stmt, out: &mut Vec<&'a Expr>) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                expr_fnlits(v, out);
            }
        }
        Stmt::Assign { values, .. } => {
            for v in values {
                expr_fnlits(v, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_fnlits(expr, out),
        Stmt::Send { chan, value, .. } => {
            expr_fnlits(chan, out);
            expr_fnlits(value, out);
        }
        Stmt::If { cond, .. } => expr_fnlits(cond, out),
        Stmt::For { kind, .. } => match kind {
            ForKind::While { cond } => expr_fnlits(cond, out),
            ForKind::Range { lo, hi, .. } => {
                expr_fnlits(lo, out);
                expr_fnlits(hi, out);
            }
            ForKind::Iterate { expr, .. } => expr_fnlits(expr, out),
            ForKind::Infinite => {}
        },
        Stmt::Return { values, .. } => {
            for v in values {
                expr_fnlits(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_fnlits(callee, out);
            for a in args {
                expr_fnlits(&a.value, out);
            }
        }
        Stmt::Managed { .. } => {}
        Stmt::Match { subject, arms, .. } => {
            expr_fnlits(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    expr_fnlits(g, out);
                }
            }
        }
        Stmt::Defer { call, .. } => expr_fnlits(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } | Stmt::Block { .. } => {}
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        expr_fnlits(chan, out);
                        expr_fnlits(value, out);
                    }
                    SelectArmKind::Recv { chan, .. } => expr_fnlits(chan, out),
                    SelectArmKind::Default => {}
                }
            }
        }
    }
}

/// Collect every `FnLit` node reachable through `expr`'s own expression
/// structure — the node itself is pushed, its body is NOT descended (the
/// caller recurses into bodies through `own_fnlits` + flatten).
pub(crate) fn expr_fnlits<'e>(expr: &'e Expr, out: &mut Vec<&'e Expr>) {
    match expr {
        Expr::FnLit { .. } => out.push(expr),
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    expr_fnlits(e, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_fnlits(lhs, out);
            expr_fnlits(rhs, out);
        }
        Expr::Unary { operand, .. } => expr_fnlits(operand, out),
        Expr::AddrOf { operand, .. } => expr_fnlits(operand, out),
        Expr::Move { operand, .. } => expr_fnlits(operand, out),
        Expr::Try { operand, .. } => expr_fnlits(operand, out),
        Expr::Receive { chan, .. } => expr_fnlits(chan, out),
        Expr::Index { base, index, .. } => {
            expr_fnlits(base, out);
            expr_fnlits(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_fnlits(base, out);
            if let Some(lo) = lo {
                expr_fnlits(lo, out);
            }
            if let Some(hi) = hi {
                expr_fnlits(hi, out);
            }
        }
        Expr::Field { base, .. } => expr_fnlits(base, out),
        Expr::Call { callee, args, .. } => {
            expr_fnlits(callee, out);
            for a in args {
                expr_fnlits(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_fnlits(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                expr_fnlits(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_fnlits(k, out);
                expr_fnlits(v, out);
            }
        }
        Expr::Paren { inner, .. } => expr_fnlits(inner, out),
    }
}

pub(crate) fn expr_calls<'e>(expr: &'e Expr, out: &mut Vec<CallRef<'e>>) {
    match expr {
        Expr::Call { callee, args, span } => {
            out.push(CallRef {
                callee,
                args,
                span: *span,
            });
            expr_calls(callee, out);
            for a in args {
                expr_calls(&a.value, out);
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    expr_calls(e, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_calls(lhs, out);
            expr_calls(rhs, out);
        }
        Expr::Unary { operand, .. } => expr_calls(operand, out),
        Expr::AddrOf { operand, .. } => expr_calls(operand, out),
        Expr::Move { operand, .. } => expr_calls(operand, out),
        Expr::Try { operand, .. } => expr_calls(operand, out),
        Expr::Receive { chan, .. } => expr_calls(chan, out),
        Expr::Index { base, index, .. } => {
            expr_calls(base, out);
            expr_calls(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_calls(base, out);
            if let Some(lo) = lo {
                expr_calls(lo, out);
            }
            if let Some(hi) = hi {
                expr_calls(hi, out);
            }
        }
        Expr::Field { base, .. } => expr_calls(base, out),
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_calls(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                expr_calls(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_calls(k, out);
                expr_calls(v, out);
            }
        }
        Expr::FnLit { .. } => {} // A5: see expr_uses / closures.rs
        Expr::Paren { inner, .. } => expr_calls(inner, out),
    }
}

// ---------------------------------------------------------------------------
// Path rendering (A3 map-slot loan bases)
// ---------------------------------------------------------------------------

/// The base path of a place, with indices dropped: `e.services[fresh]` and
/// `e.services` both render "e.services" (the container being mutated).
pub(crate) fn place_path(place: &Place) -> Option<String> {
    match &place.kind {
        PlaceKind::Ident(n) => Some(n.clone()),
        PlaceKind::Field { base, name } => Some(format!("{}.{}", place_path(base)?, name)),
        PlaceKind::Index { base, .. } => place_path(base),
        PlaceKind::AddrOf { base } => place_path(base),
    }
}

/// The base path of an expression chain (`e.services` from
/// `&e.services[existing]`'s index base).
pub(crate) fn expr_path(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Ident { name, .. } => Some(name.clone()),
        Expr::Field { base, name, .. } => Some(format!("{}.{}", expr_path(base)?, name)),
        Expr::Index { base, .. } => expr_path(base),
        Expr::Slice { base, .. } => expr_path(base),
        Expr::Paren { inner, .. } => expr_path(inner),
        Expr::Move { operand, .. } | Expr::Try { operand, .. } => expr_path(operand),
        Expr::Call { callee, args, .. }
            if matches!(crate::model::unwrap_parens(callee), Expr::Ident { name, .. } if name == "some")
                && args.len() == 1 =>
        {
            expr_path(&args[0].value)
        }
        _ => None,
    }
}

/// O8 slot identity: the literal key of an `&path[key]` loan when `path` is
/// an index-free ident/field chain, so two different keys on the same base
/// are provably distinct slots. `None` means the slot may be any slot.
pub(crate) fn addr_of_index_slot_key(value: &Expr) -> Option<String> {
    let Expr::Index { base, index, .. } = as_addr_of(value)? else {
        return None;
    };
    if !expr_is_index_free(base) {
        return None;
    }
    lit_index_key(index)
}

/// A field store `s[k].f = v` (or `s[k].f.g op= v`) writes into one slot of
/// `s` without inserting: the base path and literal key of that slot.
pub(crate) fn place_field_store_slot(place: &Place) -> Option<(String, Option<String>)> {
    if !matches!(place.kind, PlaceKind::Field { .. }) {
        return None;
    }
    let mut cur = place;
    loop {
        match &cur.kind {
            PlaceKind::Field { base, .. } => cur = base,
            PlaceKind::Index { base, index } => {
                let key = if place_is_index_free(base) {
                    lit_index_key(index)
                } else {
                    None
                };
                return Some((place_path(base)?, key));
            }
            _ => return None,
        }
    }
}

fn lit_index_key(index: &Expr) -> Option<String> {
    match crate::model::unwrap_parens(index) {
        e @ Expr::Lit { .. } => Some(e.render()),
        _ => None,
    }
}

fn expr_is_index_free(e: &Expr) -> bool {
    match e {
        Expr::Ident { .. } => true,
        Expr::Field { base, .. } => expr_is_index_free(base),
        Expr::Paren { inner, .. } => expr_is_index_free(inner),
        _ => false,
    }
}

fn place_is_index_free(p: &Place) -> bool {
    match &p.kind {
        PlaceKind::Ident(_) => true,
        PlaceKind::Field { base, .. } => place_is_index_free(base),
        _ => false,
    }
}

/// `svc := &e.services[existing]` — extract (defined-name-agnostic) the base
/// path of an `&path[key]`-shaped value, if it is one.
pub(crate) fn addr_of_index_base_path(value: &Expr) -> Option<String> {
    let operand = as_addr_of(value)?;
    match operand {
        Expr::Index { base, .. } => expr_path(base),
        _ => None,
    }
}
