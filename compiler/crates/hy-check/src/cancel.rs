//! cancel.rs — W-006 spike (#352): cancellation-safety lint (HYB-E8002).
//!
//! Opt-in (`hyb check --report-cancel` / `--deny-cancel`); never part of
//! `check_file`, so Tier-0 verdicts are untouched. Definitions and the claim
//! this pass backs live in spikes/w006-cancel-safety/RESULTS.md; in short:
//!
//!   * `close(ch)` is the only cancellation primitive in checked Hybrid
//!     (`context` is not on the curated surface), and it unblocks receivers
//!     only — never a sender.
//!   * A spawn argument is a *token* when the spawner closes that same stable
//!     binding (declared once, never assigned) or holds it as a token itself.
//!   * In a task, every blocking site (send, receive, `for … in` over a
//!     channel, `select` without `default`) must observe a token or a timer;
//!     anything else is HYB-E8002.
//!
//! Loop back-edges need no fixpoint: token-ness is a flow-insensitive must
//! property of stable bindings and sites are classified locally. The only
//! fixpoint is the memoized worklist over (fn, token set, mode) contexts.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, CallArg, Decl, Expr, File, FnDecl, ForKind, InterpPart, LocalKind, MatchPattern,
    PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

/// Result of the cancellation pass over one file.
#[derive(Debug, Default)]
pub struct CancelReport {
    /// HYB-E8002 warnings, source order, one per blocking site.
    pub diagnostics: Vec<Diagnostic>,
    /// Distinct (fn, token set, mode) contexts analyzed — the cost metric.
    pub contexts: usize,
    /// Foreign calls met in task code: outside the claim, counted only.
    pub foreign_calls: usize,
}

/// Run the cancellation pass over a parsed file.
pub fn check_cancel(file: &File) -> CancelReport {
    let fns: Vec<&FnDecl> = file.fns().filter(|f| f.body.is_some()).collect();
    let facts: Vec<Facts> = fns.iter().map(|f| Facts::of(f, &fns)).collect();
    let mut a = Analysis {
        file,
        fns: &fns,
        facts: &facts,
        nested_chan: file_has_nested_chan(file) || facts.iter().any(|f| f.nested_chan),
        chan_fields: chan_fields(file),
        seen: HashSet::new(),
        queue: VecDeque::new(),
        findings: HashMap::new(),
        foreign_calls: 0,
    };
    for (i, f) in fns.iter().enumerate() {
        if f.receiver.is_none() && (f.name == "main" || f.name == "init" || f.is_pub) {
            a.push(i, BTreeSet::new(), false, None);
        }
    }
    while let Some(c) = a.queue.pop_front() {
        a.run(&c);
    }
    let mut diagnostics: Vec<Diagnostic> = a.findings.into_values().collect();
    diagnostics.sort_by_key(|d| (d.span.start.line, d.span.start.col));
    CancelReport {
        diagnostics,
        contexts: a.seen.len(),
        foreign_calls: a.foreign_calls,
    }
}

/// Flow-insensitive per-function facts (one walk, reused by every context).
#[derive(Default)]
struct Facts<'a> {
    decls: HashMap<String, usize>,
    assigned: HashSet<String>,
    closed: HashSet<String>,
    timers: HashSet<String>,
    chans: HashSet<String>,
    /// Locals of known non-channel type (params, literals, `make` of others).
    not_chans: HashSet<String>,
    closures: HashMap<String, &'a Block>,
    /// A local type / `make` operand spells a channel inside a container.
    nested_chan: bool,
}

impl<'a> Facts<'a> {
    fn of(f: &'a FnDecl, fns: &[&FnDecl]) -> Facts<'a> {
        let mut fx = Facts::default();
        if let Some(r) = &f.receiver {
            fx.declare(&r.name);
            fx.not_chans.insert(r.name.clone());
        }
        for p in &f.params {
            fx.declare(&p.name);
            fx.type_name(&p.name, &p.ty);
        }
        for r in &f.results {
            if let Some(n) = &r.name {
                fx.declare(n);
                fx.type_name(n, &r.ty);
            }
        }
        fx.block(f.body.as_ref().unwrap(), fns);
        fx
    }

    fn declare(&mut self, n: &str) {
        if n != "_" {
            *self.decls.entry(n.to_string()).or_default() += 1;
        }
    }

    fn type_name(&mut self, n: &str, ty: &TypeExpr) {
        if matches!(ty, TypeExpr::Chan { .. }) {
            self.chans.insert(n.to_string());
        } else {
            self.not_chans.insert(n.to_string());
        }
    }

    fn stable(&self, n: &str) -> bool {
        self.decls.get(n) == Some(&1) && !self.assigned.contains(n)
    }

    fn block(&mut self, b: &'a Block, fns: &[&FnDecl]) {
        for s in &b.stmts {
            self.stmt(s, fns);
        }
    }

    fn stmt(&mut self, s: &'a Stmt, fns: &[&FnDecl]) {
        match s {
            Stmt::Local(l) => {
                for n in &l.names {
                    self.declare(n);
                }
                if let Some(ty) = &l.ty {
                    self.nested_chan |= type_nested_chan(ty, false);
                    for n in &l.names {
                        self.type_name(n, ty);
                    }
                }
                if l.names.len() == 1 && l.values.len() == 1 && l.kind != LocalKind::Const {
                    let n = &l.names[0];
                    match strip(&l.values[0]) {
                        e if is_timer_call(e) => {
                            self.timers.insert(n.clone());
                            self.chans.insert(n.clone());
                        }
                        Expr::FnLit { body, .. } => {
                            self.closures.insert(n.clone(), body);
                            self.not_chans.insert(n.clone());
                        }
                        Expr::Call { callee, args, .. } => match (strip(callee), args.first()) {
                            (Expr::Ident { name, .. }, Some(a)) if name == "make" => {
                                self.nested_chan |= render_nested_chan(&a.value.render());
                                if is_chan_type_arg(&a.value) {
                                    self.chans.insert(n.clone());
                                } else {
                                    self.not_chans.insert(n.clone());
                                }
                            }
                            (Expr::Ident { name, .. }, _) => match fn_result(fns, name) {
                                Some(true) => {
                                    self.chans.insert(n.clone());
                                }
                                Some(false) => {
                                    self.not_chans.insert(n.clone());
                                }
                                None => {}
                            },
                            _ => {}
                        },
                        Expr::Lit { .. }
                        | Expr::Interp { .. }
                        | Expr::SliceLit { .. }
                        | Expr::MapLit { .. }
                        | Expr::StructLit { .. }
                        | Expr::Binary { .. } => {
                            self.not_chans.insert(n.clone());
                        }
                        _ => {}
                    }
                }
                for v in &l.values {
                    self.expr(v, fns);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    if let PlaceKind::Ident(n) = &t.kind {
                        self.assigned.insert(n.clone());
                    }
                }
                for v in values {
                    self.expr(v, fns);
                }
            }
            Stmt::Expr { expr, .. } => self.expr(expr, fns),
            Stmt::Send { chan, value, .. } => {
                self.expr(chan, fns);
                self.expr(value, fns);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.stmt(i, fns);
                }
                self.expr(cond, fns);
                self.block(then, fns);
                if let Some(o) = otherwise {
                    self.stmt(o, fns);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.expr(cond, fns),
                    ForKind::Range { var, lo, hi, .. } => {
                        self.declare(var);
                        self.not_chans.insert(var.clone());
                        self.expr(lo, fns);
                        self.expr(hi, fns);
                    }
                    ForKind::Iterate { vars, expr } => {
                        for v in vars {
                            self.declare(v);
                        }
                        self.expr(expr, fns);
                    }
                }
                self.block(body, fns);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.expr(v, fns);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.expr(callee, fns);
                self.args(args, fns);
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => self.block(body, fns),
            Stmt::Match { subject, arms, .. } => {
                self.expr(subject, fns);
                for arm in arms {
                    if let MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        self.declare(b);
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g, fns);
                    }
                    self.block(&arm.body, fns);
                }
            }
            Stmt::Defer { call, .. } => self.expr(call, fns),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.expr(chan, fns);
                            self.expr(value, fns);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            for n in names {
                                self.declare(n);
                            }
                            self.expr(chan, fns);
                        }
                        SelectArmKind::Default => {}
                    }
                    self.block(&arm.body, fns);
                }
            }
        }
    }

    fn args(&mut self, args: &'a [CallArg], fns: &[&FnDecl]) {
        for a in args {
            self.expr(&a.value, fns);
        }
    }

    fn expr(&mut self, e: &'a Expr, fns: &[&FnDecl]) {
        if let Expr::Call { callee, args, .. } = e {
            if let (Expr::Ident { name, .. }, Some(a)) = (strip(callee), args.first()) {
                if name == "close" {
                    if let Expr::Ident { name: ch, .. } = strip(&a.value) {
                        self.closed.insert(ch.clone());
                    }
                }
            }
        }
        if let Expr::FnLit { params, .. } = e {
            for p in params {
                self.declare(&p.name);
                self.type_name(&p.name, &p.ty);
            }
        }
        for n in children(e) {
            match n {
                Node::E(x) => self.expr(x, fns),
                Node::B(b) => self.block(b, fns),
            }
        }
    }
}

/// One analysis context: function index, token set, task (vs root) mode.
#[derive(Clone)]
struct Ctx {
    f: usize,
    tokens: BTreeSet<String>,
    task: bool,
    /// Where the task was spawned (for the diagnostic note).
    origin: Option<(String, Span)>,
}

struct Analysis<'a> {
    file: &'a File,
    fns: &'a [&'a FnDecl],
    facts: &'a [Facts<'a>],
    nested_chan: bool,
    chan_fields: HashSet<String>,
    seen: HashSet<(usize, BTreeSet<String>, bool)>,
    queue: VecDeque<Ctx>,
    findings: HashMap<(u32, u32), Diagnostic>,
    foreign_calls: usize,
}

impl<'a> Analysis<'a> {
    fn push(
        &mut self,
        f: usize,
        tokens: BTreeSet<String>,
        task: bool,
        origin: Option<(String, Span)>,
    ) {
        if self.seen.insert((f, tokens.clone(), task)) {
            self.queue.push_back(Ctx {
                f,
                tokens,
                task,
                origin,
            });
        }
    }

    fn run(&mut self, c: &Ctx) {
        let body = self.fns[c.f].body.as_ref().unwrap();
        self.block(c, body);
    }

    fn fx(&self, c: &Ctx) -> &'a Facts<'a> {
        &self.facts[c.f]
    }

    fn is_token(&self, c: &Ctx, e: &Expr) -> bool {
        matches!(strip(e), Expr::Ident { name, .. } if c.tokens.contains(name))
    }

    fn is_timer(&self, c: &Ctx, e: &Expr) -> bool {
        let e = strip(e);
        is_timer_call(e)
            || matches!(e, Expr::Ident { name, .. }
                if self.fx(c).timers.contains(name) && self.fx(c).stable(name))
    }

    /// Can `e` (a `for … in` operand) be a channel? Unknown ⇒ only when the
    /// file can hide a channel behind a container type (fail-closed).
    fn may_be_chan(&self, c: &Ctx, e: &Expr) -> bool {
        let fx = self.fx(c);
        match strip(e) {
            Expr::Ident { name, .. } => {
                fx.chans.contains(name) || (!fx.not_chans.contains(name) && self.nested_chan)
            }
            Expr::Call { callee, .. } => match strip(callee) {
                Expr::Ident { name, .. } => fn_result(self.fns, name).unwrap_or(false),
                callee => is_timer_call(e) || (self.nested_chan && !is_pkg_call(fx, callee)),
            },
            Expr::Field { name, .. } => self.chan_fields.contains(name),
            Expr::Lit { .. }
            | Expr::SliceLit { .. }
            | Expr::MapLit { .. }
            | Expr::Interp { .. } => false,
            _ => self.nested_chan,
        }
    }

    fn report(&mut self, c: &Ctx, span: Span, msg: String) {
        if !c.task {
            return;
        }
        let toks: Vec<&str> = c.tokens.iter().map(|s| s.as_str()).collect();
        let mut notes = vec![format!(
            "task context: `{}` with cancellation tokens {{{}}}",
            self.fns[c.f].name,
            toks.join(", ")
        )];
        if let Some((spawned, at)) = &c.origin {
            notes.push(format!("spawned as `task {}` at {}", spawned, at.start));
        }
        notes.push(
            "close() unblocks receivers only: after cancellation this site waits on something no token can wake (W-006, spikes/w006-cancel-safety)".to_string(),
        );
        self.findings
            .entry((span.start.line, span.start.col))
            .or_insert_with(|| {
                Diagnostic::warning("HYB-E8002", self.file.path.clone(), span, msg)
                    .with_notes(notes)
            });
    }

    fn block(&mut self, c: &Ctx, b: &Block) {
        for s in &b.stmts {
            self.stmt(c, s);
        }
    }

    fn stmt(&mut self, c: &Ctx, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.expr(c, v);
                }
            }
            Stmt::Assign { values, .. } => {
                for v in values {
                    self.expr(c, v);
                }
            }
            Stmt::Expr { expr, .. } => self.expr(c, expr),
            Stmt::Send {
                chan, value, span, ..
            } => {
                self.expr(c, chan);
                self.expr(c, value);
                self.report(
                    c,
                    *span,
                    format!(
                        "send on `{}` can block forever after cancellation — close() never unblocks a sender",
                        chan.render()
                    ),
                );
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.stmt(c, i);
                }
                self.expr(c, cond);
                self.block(c, then);
                if let Some(o) = otherwise {
                    self.stmt(c, o);
                }
            }
            Stmt::For { kind, body, span } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.expr(c, cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.expr(c, lo);
                        self.expr(c, hi);
                    }
                    ForKind::Iterate { expr, .. } => {
                        self.expr(c, expr);
                        if self.may_be_chan(c, expr)
                            && !self.is_token(c, expr)
                            && !self.is_timer(c, expr)
                        {
                            self.report(
                                c,
                                *span,
                                format!(
                                    "`for … in {}` can block forever after cancellation — it is not a cancellation token",
                                    expr.render()
                                ),
                            );
                        }
                    }
                }
                self.block(c, body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.expr(c, v);
                }
            }
            Stmt::Task { callee, args, span } => {
                for a in args {
                    self.expr(c, &a.value);
                }
                self.spawn(c, callee, args, *span);
            }
            // Outside the claim: the managed contract is Go's, not Tier-0's.
            Stmt::Managed { .. } => {}
            Stmt::Match { subject, arms, .. } => {
                self.expr(c, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.expr(c, g);
                    }
                    self.block(c, &arm.body);
                }
            }
            Stmt::Defer { call, .. } => self.expr(c, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => self.block(c, block),
            Stmt::Select { arms, span } => {
                let observable = arms.iter().any(|a| match &a.kind {
                    SelectArmKind::Default => true,
                    SelectArmKind::Recv { chan, .. } => {
                        self.is_token(c, chan) || self.is_timer(c, chan)
                    }
                    SelectArmKind::Send { .. } => false,
                });
                if !observable {
                    self.report(
                        c,
                        *span,
                        "`select` can block forever after cancellation — no `default`, no token or `time.after` receive arm".to_string(),
                    );
                }
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.expr(c, chan);
                            self.expr(c, value);
                        }
                        SelectArmKind::Recv { chan, .. } => self.expr(c, chan),
                        SelectArmKind::Default => {}
                    }
                    self.block(c, &arm.body);
                }
            }
        }
    }

    fn expr(&mut self, c: &Ctx, e: &Expr) {
        match e {
            Expr::Receive { chan, span } => {
                self.expr(c, chan);
                if !self.is_token(c, chan) && !self.is_timer(c, chan) {
                    self.report(
                        c,
                        *span,
                        format!(
                            "receive from `{}` can block forever after cancellation — it is not a cancellation token",
                            chan.render()
                        ),
                    );
                }
            }
            Expr::Call { callee, args, span } => {
                let foreign = self.call(c, callee, args, *span);
                for a in args {
                    // A closure handed to a foreign API runs on foreign
                    // goroutines: outside the claim, not this task's code.
                    if foreign && matches!(strip(&a.value), Expr::FnLit { .. }) {
                        continue;
                    }
                    self.expr(c, &a.value);
                }
            }
            // Closure literals are walked inline in the defining context.
            Expr::FnLit { body, .. } => self.block(c, body),
            _ => {
                for n in children(e) {
                    if let Node::E(x) = n {
                        self.expr(c, x);
                    }
                }
            }
        }
    }

    /// Token set a callee/spawnee receives: params at positions whose
    /// argument is a stable token (calls) or a stable token-or-closed
    /// binding (spawns; and calls in root mode, which claims nothing itself).
    fn map_tokens(
        &self,
        c: &Ctx,
        callee: usize,
        args: &[CallArg],
        spawn: bool,
    ) -> BTreeSet<String> {
        let fx = self.fx(c);
        let cfx = &self.facts[callee];
        let mut out = BTreeSet::new();
        for (p, a) in self.fns[callee].params.iter().zip(args) {
            if let Expr::Ident { name, .. } = strip(&a.value) {
                let eligible = fx.stable(name)
                    && (c.tokens.contains(name)
                        || ((spawn || !c.task) && fx.closed.contains(name)));
                if eligible && cfx.stable(&p.name) && cfx.chans.contains(&p.name) {
                    out.insert(p.name.clone());
                }
            }
        }
        out
    }

    fn resolve(&self, c: &Ctx, callee: &Expr) -> Callee {
        let fx = self.fx(c);
        match strip(callee) {
            Expr::Ident { name, .. } => {
                if fx.decls.contains_key(name) {
                    if fx.closures.contains_key(name) && fx.stable(name) {
                        Callee::Covered
                    } else {
                        Callee::Unresolved
                    }
                } else if let Some(i) = self
                    .fns
                    .iter()
                    .position(|f| f.receiver.is_none() && &f.name == name)
                {
                    Callee::Fns(vec![i])
                } else {
                    // Builtins, conversions (`int(x)`), enum constructors.
                    Callee::Covered
                }
            }
            Expr::Field { base, name, .. } => {
                if is_pkg_call(fx, callee) {
                    let bounded = matches!(strip(base), Expr::Ident { name: p, .. } if p == "time")
                        && (name == "sleep" || name == "after" || name == "tick");
                    if bounded {
                        Callee::Covered
                    } else {
                        Callee::Foreign
                    }
                } else {
                    let ms: Vec<usize> = (0..self.fns.len())
                        .filter(|&i| self.fns[i].receiver.is_some() && &self.fns[i].name == name)
                        .collect();
                    if ms.is_empty() {
                        Callee::Foreign
                    } else {
                        Callee::Fns(ms)
                    }
                }
            }
            // IIFE: the literal body is walked as the callee expression.
            Expr::FnLit { .. } => Callee::Covered,
            _ => Callee::Unresolved,
        }
    }

    /// Returns true when the callee is foreign.
    fn call(&mut self, c: &Ctx, callee: &Expr, args: &[CallArg], span: Span) -> bool {
        if let Expr::FnLit { body, .. } = strip(callee) {
            self.block(c, body);
        } else if let Expr::Field { base, .. } = strip(callee) {
            self.expr(c, base);
        } else if !matches!(strip(callee), Expr::Ident { .. }) {
            self.expr(c, callee);
        }
        match self.resolve(c, callee) {
            Callee::Covered => false,
            Callee::Foreign => {
                if c.task {
                    self.foreign_calls += 1;
                }
                true
            }
            Callee::Fns(fs) => {
                for f in fs {
                    let toks = self.map_tokens(c, f, args, false);
                    self.push(f, toks, c.task, c.origin.clone());
                }
                false
            }
            Callee::Unresolved => {
                self.report(
                    c,
                    span,
                    format!(
                        "call through `{}` cannot be proven cancel-safe — its body is not visible here (fail-closed)",
                        callee.render()
                    ),
                );
                false
            }
        }
    }

    fn spawn(&mut self, c: &Ctx, callee: &Expr, args: &[CallArg], span: Span) {
        let name = callee.render();
        match self.resolve(c, callee) {
            Callee::Fns(fs) => {
                for f in fs {
                    let toks = self.map_tokens(c, f, args, true);
                    self.push(f, toks, true, Some((name.clone(), span)));
                }
            }
            Callee::Foreign => self.foreign_calls += 1,
            Callee::Covered => {
                let fx = self.fx(c);
                let body = match strip(callee) {
                    Expr::FnLit { body, .. } => Some(body),
                    Expr::Ident { name, .. } => fx.closures.get(name).copied(),
                    _ => None,
                };
                if let Some(body) = body {
                    // A spawned closure captures the spawner's bindings: its
                    // tokens are the spawner's tokens plus what it closes.
                    let mut tokens = c.tokens.clone();
                    tokens.extend(
                        fx.closed
                            .iter()
                            .filter(|n| fx.stable(n) && fx.chans.contains(*n))
                            .cloned(),
                    );
                    let child = Ctx {
                        f: c.f,
                        tokens,
                        task: true,
                        origin: Some((name, span)),
                    };
                    self.block(&child, body);
                }
            }
            Callee::Unresolved => {
                let child = Ctx {
                    task: true,
                    ..c.clone()
                };
                self.report(
                    &child,
                    span,
                    format!(
                        "`task {}` spawns code whose body is not visible here (fail-closed)",
                        name
                    ),
                );
            }
        }
    }
}

enum Callee {
    /// Never blocks, or its body is walked elsewhere in this context.
    Covered,
    /// Foreign package/method: outside the claim.
    Foreign,
    /// Hybrid functions/methods to analyze under mapped tokens.
    Fns(Vec<usize>),
    /// Body not visible: fail-closed.
    Unresolved,
}

fn strip(e: &Expr) -> &Expr {
    match e {
        Expr::Paren { inner, .. } => strip(inner),
        _ => e,
    }
}

fn is_timer_call(e: &Expr) -> bool {
    matches!(strip(e), Expr::Call { callee, .. }
        if matches!(strip(callee), Expr::Field { base, name, .. }
            if (name == "after" || name == "tick")
                && matches!(strip(base), Expr::Ident { name: p, .. } if p == "time")))
}

/// `pkg.f(...)` where `pkg` is not a binding of this function.
fn is_pkg_call(fx: &Facts<'_>, callee: &Expr) -> bool {
    matches!(strip(callee), Expr::Field { base, .. }
        if matches!(strip(base), Expr::Ident { name, .. } if !fx.decls.contains_key(name)))
}

/// `make(chan T)` — the parser carries the type operand as an expression.
fn is_chan_type_arg(e: &Expr) -> bool {
    e.render().trim_start().starts_with("chan") || e.render().trim_start().starts_with("<-chan")
}

/// Some(true/false) when `name` is a file fn with one result of known
/// chan-ness; None when it is not a file fn.
fn fn_result(fns: &[&FnDecl], name: &str) -> Option<bool> {
    let f = fns
        .iter()
        .find(|f| f.receiver.is_none() && f.name == name)?;
    Some(f.results.len() == 1 && matches!(f.results[0].ty, TypeExpr::Chan { .. }))
}

fn chan_fields(file: &File) -> HashSet<String> {
    file.decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct(s) => Some(s),
            _ => None,
        })
        .flat_map(|s| s.fields.iter())
        .filter(|f| matches!(f.ty, TypeExpr::Chan { .. }))
        .map(|f| f.name.clone())
        .collect()
}

/// Is `t` a channel nested inside a container (or a chan of chans)?
fn type_nested_chan(t: &TypeExpr, nested: bool) -> bool {
    match t {
        TypeExpr::Chan { elem, .. } => nested || type_nested_chan(elem, true),
        TypeExpr::Pointer { to: x, .. }
        | TypeExpr::Slice { elem: x, .. }
        | TypeExpr::Array { elem: x, .. }
        | TypeExpr::Option { elem: x, .. } => type_nested_chan(x, true),
        TypeExpr::Map { key, value, .. } => {
            type_nested_chan(key, true) || type_nested_chan(value, true)
        }
        TypeExpr::Fn {
            params, results, ..
        } => {
            params.iter().any(|p| type_nested_chan(&p.ty, true))
                || results.iter().any(|r| type_nested_chan(r, true))
        }
        TypeExpr::Named { .. } => false,
    }
}

/// A rendered type operand (`make([]chan int, n)`) spelling a nested chan.
fn render_nested_chan(r: &str) -> bool {
    let r = r.trim_start();
    r.matches("chan").count() > 1
        || (r.contains("chan") && !r.starts_with("chan") && !r.starts_with("<-chan"))
}

/// Does a declaration signature spell a nested channel? If neither this nor
/// any local does, an untyped `for … in` operand cannot be a channel reached
/// through an element or a receive.
fn file_has_nested_chan(file: &File) -> bool {
    file.decls.iter().any(|d| match d {
        Decl::Struct(s) => s.fields.iter().any(|f| type_nested_chan(&f.ty, false)),
        Decl::TypeAlias(a) => type_nested_chan(&a.ty, false),
        Decl::Fn(f) => {
            f.params.iter().any(|p| type_nested_chan(&p.ty, false))
                || f.results.iter().any(|r| type_nested_chan(&r.ty, false))
        }
        _ => false,
    })
}

enum Node<'a> {
    E(&'a Expr),
    B(&'a Block),
}

/// The direct sub-expressions (and closure-literal bodies) of `e`.
fn children<'a>(e: &'a Expr) -> Vec<Node<'a>> {
    if let Expr::FnLit { body, .. } = e {
        return vec![Node::B(body)];
    }
    let mut v = Vec::new();
    let mut f = |x: &'a Expr| v.push(Node::E(x));
    match e {
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    f(x);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            f(lhs);
            f(rhs);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. } => f(operand),
        Expr::Receive { chan, .. } => f(chan),
        Expr::Index { base, index, .. } => {
            f(base);
            f(index);
        }
        Expr::Slice { base, lo, hi, .. } => {
            f(base);
            if let Some(x) = lo {
                f(x);
            }
            if let Some(x) = hi {
                f(x);
            }
        }
        Expr::Field { base, .. } => f(base),
        Expr::Call { callee, args, .. } => {
            f(callee);
            for a in args {
                f(&a.value);
            }
        }
        Expr::StructLit { fields, .. } => {
            for x in fields {
                f(&x.value);
            }
        }
        Expr::SliceLit { items, .. } => {
            for x in items {
                f(x);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                f(k);
                f(v);
            }
        }
        Expr::FnLit { .. } => {}
        Expr::Paren { inner, .. } => f(inner),
    }
    v
}
