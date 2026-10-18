//! W-006 (#352): opt-in cancellation-safety lint, HYB-E8002.
//! Spec and results: spikes/w006-cancel-safety/RESULTS.md.
//!
//! `close(ch)` is checked Hybrid's only cancel primitive and wakes receivers
//! only. A spawn arg is a *token* if the spawner closes it (stable binding)
//! or holds it as a token. In a task, every send / receive / channel
//! `for … in` / default-less `select` must observe a token or live timer.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use hy_diag::{Diagnostic, Pos, Span};
use hy_syntax::{
    Block, CallArg, Decl, Expr, File, FnDecl, ForKind, InterpPart, LocalKind, MatchPattern, Place,
    PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

/// Result of the cancellation pass over one file.
#[derive(Debug, Default)]
pub struct CancelReport {
    /// HYB-E8002 warnings, one per site, source order.
    pub diagnostics: Vec<Diagnostic>,
    /// Contexts analyzed (cost metric).
    pub contexts: usize,
    /// Foreign calls in task code (outside the claim).
    pub foreign_calls: usize,
    /// Blocking sites in tasks nothing cancels (outside the claim).
    pub uncancelled_sites: usize,
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
        chan_fields: fields_where(file, |t| matches!(t, TypeExpr::Chan { .. })),
        fn_fields: fields_where(file, |t| matches!(t, TypeExpr::Fn { .. })),
        seen: HashSet::new(),
        queue: VecDeque::new(),
        findings: HashMap::new(),
        foreign_calls: 0,
        uncancelled: HashSet::new(),
        scope: Vec::new(),
    };
    for (i, f) in fns.iter().enumerate() {
        if f.receiver.is_none() && (f.name == "main" || f.name == "init" || f.is_pub) {
            a.push(i, BTreeSet::new(), false, false, None);
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
        uncancelled_sites: a.uncancelled.len(),
    }
}

/// Flow-insensitive per-fn facts, shared by all contexts.
#[derive(Default)]
struct Facts<'a> {
    decls: HashMap<String, usize>,
    assigned: HashSet<String>,
    closed: HashSet<String>,
    timers: HashMap<String, Timer>,
    /// Enclosing loop/closure bodies (block starts).
    scope: Vec<Pos>,
    chans: HashSet<String>,
    /// Bindings known not to be channels.
    not_chans: HashSet<String>,
    closures: HashMap<String, &'a Block>,
    /// Single-value initializers, to type untyped locals.
    inits: HashMap<String, &'a Expr>,
    /// Element-like bindings (loop vars, received values, match bindings).
    elems: HashSet<String>,
    /// Wait sites per channel name (receive, select arm, `for … in`).
    waits: HashMap<String, usize>,
    /// A local type spells a channel inside a container.
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
                    self.inits.insert(n.clone(), &l.values[0]);
                    match strip(&l.values[0]) {
                        e if is_timer_call(e) => {
                            let t = if is_tick_call(e) {
                                Timer::Periodic
                            } else {
                                Timer::Once(self.scope.last().copied())
                            };
                            self.timers.insert(n.clone(), t);
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
                    for x in place_exprs(t) {
                        self.expr(x, fns);
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
                            self.elems.insert(v.clone());
                        }
                        self.wait(expr);
                        self.expr(expr, fns);
                    }
                }
                self.scope.push(body.span.start);
                self.block(body, fns);
                self.scope.pop();
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
                        self.elems.insert(b.clone());
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
                                self.elems.insert(n.clone());
                            }
                            self.wait(chan);
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

    fn wait(&mut self, ch: &Expr) {
        if let Expr::Ident { name, .. } = strip(ch) {
            *self.waits.entry(name.clone()).or_default() += 1;
        }
    }

    fn expr(&mut self, e: &'a Expr, fns: &[&FnDecl]) {
        if let Expr::Receive { chan, .. } = e {
            self.wait(chan);
        }
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
                Node::B(b) => {
                    self.scope.push(b.span.start);
                    self.block(b, fns);
                    self.scope.pop();
                }
            }
        }
    }
}

/// `time.after` fires once: it bounds a wait only in the body that bound it
/// (`None` = fn top), since a back-edge re-waits on a drained timer.
#[derive(Clone, Copy, PartialEq)]
enum Timer {
    Periodic,
    Once(Option<Pos>),
}

/// Analysis context: fn, tokens, task vs root mode.
#[derive(Clone)]
struct Ctx {
    f: usize,
    tokens: BTreeSet<String>,
    task: bool,
    /// In scope: holds a token, spawner closes a channel, or spawned by an
    /// in-scope task. Out-of-scope sites are counted, not reported.
    cancellable: bool,
    /// Spawn site, for the diagnostic note.
    origin: Option<(String, Span)>,
}

struct Analysis<'a> {
    file: &'a File,
    fns: &'a [&'a FnDecl],
    facts: &'a [Facts<'a>],
    nested_chan: bool,
    chan_fields: HashSet<String>,
    /// Fn-typed field names: `x.f()` fails closed.
    fn_fields: HashSet<String>,
    seen: HashSet<(usize, BTreeSet<String>, bool, bool)>,
    queue: VecDeque<Ctx>,
    findings: HashMap<(u32, u32), Diagnostic>,
    foreign_calls: usize,
    uncancelled: HashSet<(u32, u32)>,
    /// Mirrors `Facts::scope`.
    scope: Vec<Pos>,
}

impl<'a> Analysis<'a> {
    fn push(
        &mut self,
        f: usize,
        tokens: BTreeSet<String>,
        task: bool,
        cancellable: bool,
        origin: Option<(String, Span)>,
    ) {
        if self.seen.insert((f, tokens.clone(), task, cancellable)) {
            self.queue.push_back(Ctx {
                f,
                tokens,
                task,
                cancellable,
                origin,
            });
        }
    }

    fn run(&mut self, c: &Ctx) {
        let body = self.fns[c.f].body.as_ref().unwrap();
        self.scope.clear();
        self.block(c, body);
    }

    /// Walk a loop/closure body (timer back-edge rule).
    fn scoped(&mut self, c: &Ctx, b: &Block) {
        self.scope.push(b.span.start);
        self.block(c, b);
        self.scope.pop();
    }

    /// Root mode: finds spawns, reports nothing.
    fn rooted(c: &Ctx) -> Ctx {
        Ctx {
            task: false,
            ..c.clone()
        }
    }

    fn fx(&self, c: &Ctx) -> &'a Facts<'a> {
        &self.facts[c.f]
    }

    fn is_token(&self, c: &Ctx, e: &Expr) -> bool {
        matches!(strip(e), Expr::Ident { name, .. } if c.tokens.contains(name))
    }

    fn is_timer(&self, c: &Ctx, e: &Expr) -> bool {
        let e = strip(e);
        if is_timer_call(e) {
            return true;
        }
        let Expr::Ident { name, .. } = e else {
            return false;
        };
        let fx = self.fx(c);
        fx.stable(name)
            && match fx.timers.get(name) {
                Some(Timer::Periodic) => true,
                Some(Timer::Once(at)) => {
                    *at == self.scope.last().copied() && fx.waits.get(name) == Some(&1)
                }
                None => false,
            }
    }

    /// May a `for … in` operand be a channel? Unknown fails closed, except
    /// element-like values, which need a nested chan type in the file.
    fn may_be_chan(&self, c: &Ctx, e: &Expr, depth: u8) -> bool {
        let fx = self.fx(c);
        match strip(e) {
            Expr::Ident { name, .. } => {
                if fx.chans.contains(name) {
                    true
                } else if fx.not_chans.contains(name) || !fx.decls.contains_key(name) {
                    false
                } else if fx.elems.contains(name) {
                    self.nested_chan
                } else {
                    match fx.inits.get(name) {
                        Some(init) if depth < 8 => self.may_be_chan(c, init, depth + 1),
                        _ => true,
                    }
                }
            }
            Expr::Call { callee, .. } => match strip(callee) {
                Expr::Ident { name, .. } => fn_result(self.fns, name).unwrap_or(false),
                f @ Expr::Field { name, .. } => {
                    if is_pkg_call(fx, f) {
                        is_timer_call(e)
                    } else {
                        let ms: Vec<&&FnDecl> = self
                            .fns
                            .iter()
                            .filter(|m| m.receiver.is_some() && &m.name == name)
                            .collect();
                        // No such method: a foreign method, result unknown.
                        ms.is_empty()
                            || ms.iter().any(|m| {
                                m.results.len() == 1
                                    && matches!(m.results[0].ty, TypeExpr::Chan { .. })
                            })
                    }
                }
                _ => true,
            },
            Expr::Field { name, .. } => self.chan_fields.contains(name),
            Expr::Lit { .. }
            | Expr::SliceLit { .. }
            | Expr::MapLit { .. }
            | Expr::StructLit { .. }
            | Expr::Interp { .. }
            | Expr::Binary { .. }
            | Expr::Unary { .. } => false,
            _ => self.nested_chan,
        }
    }

    fn report(&mut self, c: &Ctx, span: Span, msg: String) {
        if !c.task {
            return;
        }
        if !c.cancellable {
            self.uncancelled.insert((span.start.line, span.start.col));
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
            Stmt::Assign {
                targets, values, ..
            } => {
                for x in targets.iter().flat_map(place_exprs) {
                    self.expr(c, x);
                }
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
                        if self.may_be_chan(c, expr, 0)
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
                self.scoped(c, body);
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
            // Own sites outside the claim; spawns inside still analyzed.
            Stmt::Managed { body, .. } => self.block(&Self::rooted(c), body),
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
                        "`select` can block forever after cancellation — no `default`, no token receive arm, and no timer arm still live on this iteration (a `time.after` bound outside the loop fires once)".to_string(),
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
                    // Runs on foreign goroutines: root mode.
                    if let (true, Expr::FnLit { body, .. }) = (foreign, strip(&a.value)) {
                        self.scoped(&Self::rooted(c), body);
                        continue;
                    }
                    self.expr(c, &a.value);
                }
            }
            // Closures are walked inline where defined.
            Expr::FnLit { body, .. } => self.scoped(c, body),
            _ => {
                for n in children(e) {
                    if let Node::E(x) = n {
                        self.expr(c, x);
                    }
                }
            }
        }
    }

    /// Callee params receiving a stable token; spawns and root-mode calls
    /// also pass stable bindings the caller closes.
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
        // Named or spread args break positional mapping: map nothing.
        if args.iter().any(|a| a.name.is_some() || a.spread) {
            return out;
        }
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
                    // Builtins, conversions, enum constructors.
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
                    if self.fn_fields.contains(name) {
                        Callee::Unresolved
                    } else if ms.is_empty() {
                        Callee::Foreign
                    } else {
                        Callee::Fns(ms)
                    }
                }
            }
            // IIFE: body walked in `call`.
            Expr::FnLit { .. } => Callee::Covered,
            _ => Callee::Unresolved,
        }
    }

    /// Returns whether the callee is foreign.
    fn call(&mut self, c: &Ctx, callee: &Expr, args: &[CallArg], span: Span) -> bool {
        if let Expr::FnLit { body, .. } = strip(callee) {
            self.scoped(c, body);
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
                    self.push(f, toks, c.task, c.cancellable, c.origin.clone());
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

    /// Is a task spawned from `c` with `tokens` in scope?
    fn spawn_cancellable(&self, c: &Ctx, tokens: &BTreeSet<String>) -> bool {
        !tokens.is_empty() || !self.fx(c).closed.is_empty() || (c.task && c.cancellable)
    }

    fn spawn(&mut self, c: &Ctx, callee: &Expr, args: &[CallArg], span: Span) {
        let name = callee.render();
        match self.resolve(c, callee) {
            Callee::Fns(fs) => {
                for f in fs {
                    let toks = self.map_tokens(c, f, args, true);
                    let cancellable = self.spawn_cancellable(c, &toks);
                    self.push(f, toks, true, cancellable, Some((name.clone(), span)));
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
                    // Captures the spawner's bindings: inherit its tokens + closes.
                    let mut tokens = c.tokens.clone();
                    tokens.extend(
                        fx.closed
                            .iter()
                            .filter(|n| fx.stable(n) && fx.chans.contains(*n))
                            .cloned(),
                    );
                    let child = Ctx {
                        f: c.f,
                        cancellable: self.spawn_cancellable(c, &tokens),
                        tokens,
                        task: true,
                        origin: Some((name, span)),
                    };
                    self.scoped(&child, body);
                }
            }
            Callee::Unresolved => {
                let child = Ctx {
                    task: true,
                    cancellable: self.spawn_cancellable(c, &c.tokens),
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
    /// Never blocks, or walked elsewhere.
    Covered,
    /// Outside the claim.
    Foreign,
    /// Hybrid fns/methods to analyze.
    Fns(Vec<usize>),
    /// Body not visible: fail closed.
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

/// Expressions inside an assignment target (`m[<-c] = v`).
fn place_exprs(p: &Place) -> Vec<&Expr> {
    match &p.kind {
        PlaceKind::Ident(_) => vec![],
        PlaceKind::Index { base, index } => {
            let mut v = place_exprs(base);
            v.push(index);
            v
        }
        PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => place_exprs(base),
    }
}

fn is_tick_call(e: &Expr) -> bool {
    matches!(strip(e), Expr::Call { callee, .. }
        if matches!(strip(callee), Expr::Field { name, .. } if name == "tick"))
}

/// `pkg.f(...)` where `pkg` is not a binding of this function.
fn is_pkg_call(fx: &Facts<'_>, callee: &Expr) -> bool {
    matches!(strip(callee), Expr::Field { base, .. }
        if matches!(strip(base), Expr::Ident { name, .. } if !fx.decls.contains_key(name)))
}

/// `make(chan T)`: the type operand is parsed as an expression.
fn is_chan_type_arg(e: &Expr) -> bool {
    e.render().trim_start().starts_with("chan") || e.render().trim_start().starts_with("<-chan")
}

/// Whether file fn `name` returns a channel; `None` if no such fn.
fn fn_result(fns: &[&FnDecl], name: &str) -> Option<bool> {
    let f = fns
        .iter()
        .find(|f| f.receiver.is_none() && f.name == name)?;
    Some(f.results.len() == 1 && matches!(f.results[0].ty, TypeExpr::Chan { .. }))
}

/// Struct field names (any struct) whose type matches.
fn fields_where(file: &File, pred: fn(&TypeExpr) -> bool) -> HashSet<String> {
    file.decls
        .iter()
        .filter_map(|d| match d {
            Decl::Struct(s) => Some(s),
            _ => None,
        })
        .flat_map(|s| s.fields.iter())
        .filter(|f| pred(&f.ty))
        .map(|f| f.name.clone())
        .collect()
}

/// Is `t` a channel nested in a container?
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

/// Rendered `make` operand spelling a nested chan.
fn render_nested_chan(r: &str) -> bool {
    let r = r.trim_start();
    r.matches("chan").count() > 1
        || (r.contains("chan") && !r.starts_with("chan") && !r.starts_with("<-chan"))
}

/// Does a declaration spell a nested chan? (Gates `may_be_chan` fallback.)
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

/// Direct sub-expressions and closure bodies of `e`.
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
