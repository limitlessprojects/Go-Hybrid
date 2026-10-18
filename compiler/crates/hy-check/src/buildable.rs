//! buildable.rs — issue #134: an accepted program must emit buildable Go.
//!
//! Go's front-end rejects four declaration shapes that `hyb check` used to
//! accept with 0 diagnostics, so the emitted module could never build:
//!
//!   1. an unused `:=` binding — `x := 5` with no read of `x` is
//!      `declared and not used`; `_ := 5` (and `_, _ := ...`) declares no
//!      new variable at all (`no new variables on left side of :=`);
//!   2. an unused import — the emitter carries every declared Go import
//!      into the header, and Go rejects `imported and not used`;
//!   3. a repeated name on the left of one `:=` — `x, x := 1, 2` is
//!      `x repeated on left side of :=`;
//!   4. unused function-local `var` or `const` variables emitted as
//!      Go locals — rejected with `declared and not used`.
//!
//! All four reject fail-closed as HYB-E1002 (registry rule `BUILDABLE`):
//! what the checker accepts must survive `go build`.
//!
//! Issue #425 extends this file-level buildability pass to operand types
//! and binding-shaped builtin calls:
//!   * A. non-boolean `if` / `for` conditions (HYB-E1002)
//!   * B. operator / operand type mismatches (`!x` on non-bool, `-s` on
//!        non-numeric, `x + s` mismatched types, `x && true` on non-bool,
//!        `b < false` ordered compare on non-ordered type, `s++` on
//!        non-numeric) (HYB-E1002)
//!   * C. indexing or slicing a non-indexable / non-sliceable value
//!        (`x[0]`, `x[0:1]`) (HYB-E1002)
//!   * D. calling a resolved non-function value (`x()`) (HYB-E1002)
//!   * E. builtin operand-kind misuse through a binding (`len(x)`,
//!        `close(x)`, `cap(m)`, `delete(xs, 0)`, `append(x, 1)`)
//!        (HYB-E2005, extending #187 from literals to resolved bindings).
//!
//! The read-set is deliberately Go-faithful, not conservative-in-the-wrong-
//! direction: a name is "read" when Go's own unused-variable check counts it
//! — any mention in the function, including inside closure bodies, and
//! including `x++`, `x += 1`, `x[k] = v`, `x.f = v` and `&x` — while a bare
//! `x = v` store is NOT a read (gc reports `declared and not used` for a
//! write-only local). The same-scope re-define (`x, y := 1, 2` after
//! `x := 0`) keeps the existing variable, exactly as Go's `:=` does.
//!
//! Out of scope, documented: select-receive names and match bindings
//! (distinct source binding paths), unread `for ... in` bindings (lowered to
//! `_` at emission, issue #139), and unused `rust/` imports (they are never
//! emitted as Go imports, so they cannot break the build).

use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, Block, CallArg, Decl, Expr, File, FnDecl, ForKind, InterpPart, LitKind,
    LocalDecl, LocalKind, MatchPattern, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr, UnOp,
};
use std::collections::{HashMap, HashSet};

use crate::model::{unwrap_parens, Checker};

/// Lightweight operand-kind classification for issue #425: distinguishes
/// `Int`, `Float`, and `Bool` (which `TypeShape::Primitive` collapses) so
/// non-boolean conditions, operator/operand type mismatches, non-indexable
/// index/slice bases, non-function calls, and binding-shaped builtin misuse
/// are rejected at check time rather than failing `go build`.
/// `Unknown` stays permissive (fail-open for operand checks; concurrency
/// boundaries remain fail-closed in `sendability.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
enum OpKind {
    Unknown,
    Int,
    Float,
    Bool,
    Str,
    Slice(Box<OpKind>),
    Array(Box<OpKind>),
    Map(Box<OpKind>, Box<OpKind>),
    Chan(Box<OpKind>),
    Struct(Option<String>),
    Pointer(Box<OpKind>),
    Fn(Vec<OpKind>),
    Void,
}

impl OpKind {
    fn is_numeric(&self) -> bool {
        matches!(self, OpKind::Int | OpKind::Float)
    }

    fn label(&self) -> String {
        match self {
            OpKind::Unknown => "unknown".to_string(),
            OpKind::Int => "int".to_string(),
            OpKind::Float => "float64".to_string(),
            OpKind::Bool => "bool".to_string(),
            OpKind::Str => "string".to_string(),
            OpKind::Slice(e) => format!("[]{}", e.label()),
            OpKind::Array(e) => format!("[N]{}", e.label()),
            OpKind::Map(k, v) => format!("map[{}]{}", k.label(), v.label()),
            OpKind::Chan(e) => format!("chan {}", e.label()),
            OpKind::Struct(Some(n)) => format!("struct {}", n),
            OpKind::Struct(None) => "struct".to_string(),
            OpKind::Pointer(inner) => format!("*{}", inner.label()),
            OpKind::Fn(_) => "fn".to_string(),
            OpKind::Void => "void".to_string(),
        }
    }
}

/// One function-local binding that Go requires to be read: `:=`/`let`
/// (checked since #185), `var`/`const` (PR #475), and the elidable
/// `for`/`select` bindings (PR #536 — only diagnosed when written).
/// Tracked for both reads and writes.
struct Define {
    name: String,
    span: Span,
    kind: LocalKind,
    used: bool,
    written: bool,
    elidable: bool,
}

struct Collector<'a> {
    ck: &'a Checker<'a>,
    /// One layer per open block/closure. `Some(id)` = a checked define
    /// (`:=`/`let`/`var`/`const`, or a written elidable `for`/`select`
    /// binding); `None` = a name that shadows (params, receivers, named
    /// results, untouched `for`/select/match bindings) outside the read rule.
    scopes: Vec<HashMap<String, Option<usize>>>,
    /// Parallel lexical scope stack tracking resolved operand kinds (#425).
    kind_scopes: Vec<HashMap<String, OpKind>>,
    /// File-level `const` kinds (#425).
    file_consts: HashMap<String, OpKind>,
    defines: Vec<Define>,
    /// Every identifier mentioned in the file plus every dotted type root —
    /// the import-usage set (the emitter's declared-import header needs it).
    roots: HashSet<String>,
    diags: Vec<Diagnostic>,
    path: String,
}

/// Entry point: `check_file` calls this once per file (after the per-fn
/// passes; diagnostics are sorted into source order by the caller).
pub(crate) fn check_buildable(ck: &mut Checker<'_>) {
    let file: &File = ck.file;
    let diags = {
        let mut c = Collector {
            ck,
            scopes: Vec::new(),
            kind_scopes: Vec::new(),
            file_consts: HashMap::new(),
            defines: Vec::new(),
            roots: HashSet::new(),
            diags: Vec::new(),
            path: ck.path.clone(),
        };
        for d in &file.decls {
            if let Decl::Const(cd) = d {
                let k = cd
                    .ty
                    .as_ref()
                    .map(|t| c.kind_of_ty(t, 0))
                    .unwrap_or_else(|| c.kind_of_expr(&cd.value));
                c.file_consts.insert(cd.name.clone(), k);
            }
        }
        for d in &file.decls {
            match d {
                Decl::Import { .. } => {}
                Decl::Const(cd) => {
                    c.ty_opt(&cd.ty);
                    c.expr(&cd.value);
                }
                Decl::Struct(s) => {
                    for f in &s.fields {
                        c.ty(&f.ty);
                    }
                }
                Decl::Enum(e) => {
                    for v in &e.variants {
                        if let Some(t) = &v.payload {
                            c.ty(t);
                        }
                    }
                }
                Decl::TypeAlias(t) => c.ty(&t.ty),
                Decl::Fn(f) => c.fn_decl(f),
            }
        }
        c.check_imports(file);
        c.finish();
        c.diags
    };
    ck.diags.extend(diags);
}

impl<'a> Collector<'a> {
    // -- scope bookkeeping ---------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.kind_scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.kind_scopes.pop();
    }

    /// A name that shadows checked defines but is not itself checked.
    fn exempt(&mut self, name: &str) {
        if name != "_" {
            if let Some(top) = self.scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
        }
    }

    fn bind_kind(&mut self, name: &str, kind: OpKind) {
        if name != "_" {
            if let Some(top) = self.kind_scopes.last_mut() {
                top.insert(name.to_string(), kind);
            }
        }
    }

    fn lookup_kind(&self, name: &str) -> Option<OpKind> {
        for layer in self.kind_scopes.iter().rev() {
            if let Some(k) = layer.get(name) {
                return Some(k.clone());
            }
        }
        self.file_consts.get(name).cloned()
    }

    /// A `for`-iterate or `select`-receive binding is elided to `_` at
    /// emission when completely untouched, but if the body assigns to it
    /// (`v = ...`) without ever reading it, the emitter must keep the binding
    /// and Go rejects it as `declared and not used`.
    fn define_elidable(&mut self, name: &str, span: Span) {
        if name == "_" {
            return;
        }
        let Some(top) = self.scopes.last_mut() else {
            return;
        };
        if top.contains_key(name) {
            return;
        }
        let id = self.defines.len();
        self.defines.push(Define {
            name: name.to_string(),
            span,
            kind: LocalKind::Define,
            used: false,
            written: false,
            elidable: true,
        });
        top.insert(name.to_string(), Some(id));
    }

    /// Every identifier mention not shadowed by a local or parameter binding
    /// feeds the import-usage set.
    fn mention(&mut self, name: &str) {
        if !self
            .scopes
            .iter()
            .rev()
            .any(|layer| layer.contains_key(name))
        {
            self.roots.insert(name.to_string());
        }
    }

    /// A read of `name`: marks the innermost checked define it resolves to.
    /// A read inside a closure resolves through the closure's scope layers
    /// to the captured outer define — exactly what Go's check counts.
    fn read(&mut self, name: &str) {
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                if let Some(id) = entry {
                    self.defines[*id].used = true;
                }
                return;
            }
        }
        self.mention(name);
    }

    fn write(&mut self, name: &str) {
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                if let Some(id) = entry {
                    self.defines[*id].written = true;
                }
                return;
            }
        }
        self.mention(name);
    }

    fn check_repeated_bindings(&mut self, names: &[String], span: Span) {
        let mut seen: HashSet<&str> = HashSet::new();
        for name in names {
            if name != "_" && !seen.insert(name.as_str()) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        span,
                        format!(
                            "`{}` is repeated on the left of one `:=` — Go refuses `{} := ...` (repeated on left side of :=) (HYB-E1002)",
                            name,
                            names.join(", ")
                        ),
                    )
                    .with_notes(vec![
                        "each left-hand name must appear once; write `x, y := 1, 2`"
                            .to_string(),
                    ]),
                );
            }
        }
    }

    // -- declarations --------------------------------------------------------

    fn fn_decl(&mut self, f: &FnDecl) {
        if let Some(r) = &f.receiver {
            self.ty(&r.ty);
        }
        for p in &f.params {
            self.ty(&p.ty);
        }
        for r in &f.results {
            self.ty(&r.ty);
        }
        let Some(body) = &f.body else { return };
        self.push_scope();
        if let Some(r) = &f.receiver {
            self.exempt(&r.name);
            let rk = self.kind_of_ty(&r.ty, 0);
            self.bind_kind(&r.name, rk);
        }
        for p in &f.params {
            self.exempt(&p.name);
            let pk = self.kind_of_ty(&p.ty, 0);
            self.bind_kind(&p.name, pk);
        }
        for r in &f.results {
            if let Some(rname) = &r.name {
                self.exempt(rname);
                let rk = self.kind_of_ty(&r.ty, 0);
                self.bind_kind(rname, rk);
            }
        }
        for r in &f.results {
            if let Some(name) = &r.name {
                self.exempt(name);
            }
        }
        self.block_stmts(body);
        self.pop_scope();
    }

    fn local(&mut self, l: &LocalDecl) {
        // Only the short declaration operator needs freshness checks.
        // Every kind of fn-local declaration emits a Go binding, whose
        // name must be read somewhere in its enclosing function.
        let short_define = matches!(l.kind, LocalKind::Define | LocalKind::Let);
        let track_read = matches!(
            l.kind,
            LocalKind::Define | LocalKind::Let | LocalKind::Var | LocalKind::Const
        );
        if short_define {
            // Form 3: `x, x := 1, 2` — `x repeated on left side of :=`.
            self.check_repeated_bindings(&l.names, l.span);
            // Form 1 (the `_` half): `:=` needs at least one NEW variable.
            // `_ := 5`, `_, _ := ...` and a same-scope re-define declare
            // none — Go's `no new variables on left side of :=`.
            let fresh = l.names.iter().any(|n| {
                n != "_"
                    && !self
                        .scopes
                        .last()
                        .is_some_and(|top| top.contains_key(n.as_str()))
            });
            if !fresh {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        l.span,
                        format!(
                            "`{} := ...` declares no new variable — Go refuses it (no new variables on left side of :=) (HYB-E1002)",
                            l.names.join(", ")
                        ),
                    )
                    .with_notes(vec![
                        "`_` is not a variable: `_ := 5` can never build — write `_ = 5` to discard the value, or bind a real name".to_string(),
                        "a name already declared in this same scope is not new either".to_string(),
                    ]),
                );
            }
        }
        if let Some(t) = &l.ty {
            self.ty(t);
        }
        // RHS first: `x := x` reads the OUTER `x` (Go's scoping).
        for v in &l.values {
            self.expr(v);
        }
        // Composed resolution (pr-430 x landed pr-475): #475's track_read
        // set (Define|Let|Var|Const) drives which locals get unused-read
        // tracking; #430's operand-kind inference runs for EVERY name so
        // `kind_of_expr` resolves binding-shaped operands (issue #425).
        let kinds = self.infer_local_kinds(l);
        for (name, kind) in l.names.iter().zip(kinds) {
            self.bind_kind(name, kind);
            if !track_read {
                self.exempt(name);
                continue;
            }
            if name == "_" {
                continue;
            }
            let Some(top) = self.scopes.last_mut() else {
                continue;
            };
            if top.contains_key(name.as_str()) {
                // Same-scope re-define: Go keeps the existing variable.
                continue;
            }
            let id = self.defines.len();
            self.defines.push(Define {
                name: name.clone(),
                span: l.span,
                kind: l.kind,
                used: false,
                written: false,
                elidable: false,
            });
            top.insert(name.clone(), Some(id));
        }
    }

    fn infer_local_kinds(&self, l: &LocalDecl) -> Vec<OpKind> {
        let n = l.names.len();
        if let Some(ty) = &l.ty {
            return vec![self.kind_of_ty(ty, 0); n];
        }
        if n == 2 && l.values.len() == 1 {
            return self.multi_value_kinds(&l.values[0]);
        }
        if n == l.values.len() {
            return l.values.iter().map(|v| self.kind_of_expr(v)).collect();
        }
        vec![OpKind::Unknown; n]
    }

    fn multi_value_kinds(&self, e: &Expr) -> Vec<OpKind> {
        match unwrap_parens(e) {
            Expr::Move { operand, .. } => self.multi_value_kinds(operand),
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Map(_, v) => vec![*v, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Bool],
            },
            Expr::Receive { chan, .. } => match self.kind_of_expr(chan) {
                OpKind::Chan(elem) => vec![*elem, OpKind::Bool],
                _ => vec![OpKind::Unknown, OpKind::Bool],
            },
            Expr::Call { callee, args, .. } => {
                if let Some(res) = self.call_result_kinds(callee, args) {
                    if res.len() >= 2 {
                        return vec![res[0].clone(), res[1].clone()];
                    }
                }
                vec![OpKind::Unknown, OpKind::Unknown]
            }
            _ => vec![OpKind::Unknown, OpKind::Unknown],
        }
    }

    // -- statements ----------------------------------------------------------

    fn block_stmts(&mut self, b: &Block) {
        for s in &b.stmts {
            self.stmt(s);
        }
    }

    fn block_scoped(&mut self, b: &Block) {
        self.push_scope();
        self.block_stmts(b);
        self.pop_scope();
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => self.local(l),
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.place(t, *op);
                }
                for v in values {
                    self.expr(v);
                }
                self.check_assign_operands(targets, *op, values, *span);
            }
            Stmt::Expr { expr, .. } => self.expr(expr),
            Stmt::Send { chan, value, .. } => {
                self.expr(chan);
                self.expr(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                // Go's if-init scope covers the whole construct.
                self.push_scope();
                if let Some(i) = init {
                    self.stmt(i);
                }
                self.expr(cond);
                self.check_bool_condition(cond, "if");
                self.block_scoped(then);
                if let Some(o) = otherwise {
                    self.stmt(o);
                }
                self.pop_scope();
            }
            Stmt::For {
                kind, body, span, ..
            } => {
                self.push_scope();
                match kind {
                    ForKind::While { cond } => {
                        self.expr(cond);
                        self.check_bool_condition(cond, "for");
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        self.expr(lo);
                        self.expr(hi);
                        // The loop head always reads the counter.
                        self.exempt(var);
                        self.bind_kind(var, OpKind::Int);
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.check_repeated_bindings(vars, *span);
                        self.expr(expr);
                        let iter_kinds = self.iterate_var_kinds(expr, vars.len());
                        // Unread iterate bindings are lowered to `_` at
                        // emission (issue #139); write-only ones still
                        // reject as unused (PR #536).
                        for (v, k) in vars.iter().zip(iter_kinds) {
                            self.define_elidable(v, *span);
                            self.bind_kind(v, k);
                        }
                    }
                    ForKind::Infinite => {}
                }
                self.block_scoped(body);
                self.pop_scope();
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.expr(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
            }
            Stmt::Managed { body, .. } => self.block_scoped(body),
            Stmt::Match { subject, arms, .. } => {
                self.expr(subject);
                for arm in arms {
                    self.push_scope();
                    if let MatchPattern::Path { segments, binding } = &arm.pattern {
                        if let Some(root) = segments.first() {
                            self.mention(root);
                        }
                        if let Some(b) = binding {
                            self.define_elidable(b, subject.span());
                            self.bind_kind(b, OpKind::Unknown);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                        self.check_bool_condition(g, "match guard");
                    }
                    self.block_scoped(&arm.body);
                    self.pop_scope();
                }
            }
            Stmt::Defer { call, .. } => self.expr(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => self.block_scoped(block),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    self.push_scope();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.check_repeated_bindings(names, chan.span());
                            self.expr(chan);
                            let elem = match self.kind_of_expr(chan) {
                                OpKind::Chan(e) => *e,
                                _ => OpKind::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                self.define_elidable(n, chan.span());
                                self.bind_kind(n, if i == 0 { elem.clone() } else { OpKind::Bool });
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    self.block_scoped(&arm.body);
                    self.pop_scope();
                }
            }
        }
    }

    fn iterate_var_kinds(&self, expr: &Expr, n_vars: usize) -> Vec<OpKind> {
        match self.kind_of_expr(expr) {
            OpKind::Slice(e) | OpKind::Array(e) | OpKind::Chan(e) => {
                if n_vars >= 2 {
                    vec![OpKind::Int, *e]
                } else {
                    vec![*e]
                }
            }
            OpKind::Map(k, v) => {
                if n_vars >= 2 {
                    vec![*k, *v]
                } else {
                    vec![*v]
                }
            }
            OpKind::Str => vec![OpKind::Int; n_vars.max(1)],
            _ => vec![OpKind::Unknown; n_vars.max(1)],
        }
    }

    // -- places (assignment targets) ----------------------------------------

    fn place(&mut self, p: &Place, op: AssignOp) {
        match &p.kind {
            PlaceKind::Ident(n) => {
                // A bare `x = v` store is the ONE target form Go does not
                // count as a read; `x++`, `x += 1` and every non-ident
                // target (`x[k] = v`, `x.f = v`, `&x`) read the root.
                if matches!(op, AssignOp::Assign) {
                    self.write(n);
                } else {
                    self.read(n);
                }
            }
            PlaceKind::Index { .. } | PlaceKind::Field { .. } | PlaceKind::AddrOf { .. } => {
                self.place_read(p)
            }
        }
    }

    fn place_read(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(n) => self.read(n),
            PlaceKind::Index { base, index } => {
                self.place_read(base);
                self.expr(index);
                let bk = self.kind_of_place(base);
                let ik = self.kind_of_expr(index);
                self.check_index_kind(&base.render(), &bk, index, &ik, p.span);
            }
            PlaceKind::Field { base, .. } => self.place_read(base),
            PlaceKind::AddrOf { base } => self.place_read(base),
        }
    }

    // -- expressions and types ----------------------------------------------

    fn expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident { name, .. } => self.read(name),
            Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.expr(x);
                    }
                }
            }
            Expr::Binary {
                op, lhs, rhs, span, ..
            } => {
                self.expr(lhs);
                self.expr(rhs);
                self.check_binary_operands(*op, lhs, rhs, *span);
            }
            Expr::Unary {
                op, operand, span, ..
            } => {
                self.expr(operand);
                self.check_unary_operand(*op, operand, *span);
            }
            Expr::AddrOf { operand, .. } => self.expr(operand),
            Expr::Move { operand, .. } => self.expr(operand),
            Expr::Try { operand, .. } => self.expr(operand),
            Expr::Receive { chan, .. } => self.expr(chan),
            Expr::Index {
                base, index, span, ..
            } => {
                self.expr(base);
                self.expr(index);
                let bk = self.kind_of_expr(base);
                let ik = self.kind_of_expr(index);
                self.check_index_kind(&base.render(), &bk, index, &ik, *span);
            }
            Expr::Slice {
                base, lo, hi, span, ..
            } => {
                self.expr(base);
                if let Some(x) = lo {
                    self.expr(x);
                }
                if let Some(x) = hi {
                    self.expr(x);
                }
                let bk = self.kind_of_expr(base);
                self.check_slice_kind(base, &bk, lo.as_deref(), hi.as_deref(), *span);
            }
            Expr::Field { base, .. } => self.expr(base),
            Expr::Call {
                callee, args, span, ..
            } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
                self.check_call_operands(callee, args, *span);
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.ty(t);
                }
                for f in fields {
                    self.expr(&f.value);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.ty(t);
                }
                for i in items {
                    self.expr(i);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.ty(key);
                self.ty(value);
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.ty(&p.ty);
                }
                for r in results {
                    self.ty(&r.ty);
                }
                self.push_scope();
                for p in params {
                    self.exempt(&p.name);
                    let pk = self.kind_of_ty(&p.ty, 0);
                    self.bind_kind(&p.name, pk);
                }
                for r in results {
                    if let Some(rname) = &r.name {
                        self.exempt(rname);
                        let rk = self.kind_of_ty(&r.ty, 0);
                        self.bind_kind(rname, rk);
                    }
                }
                for r in results {
                    if let Some(name) = &r.name {
                        self.exempt(name);
                    }
                }
                self.block_stmts(body);
                self.pop_scope();
            }
            Expr::Paren { inner, .. } => self.expr(inner),
        }
    }

    fn ty_opt(&mut self, t: &Option<TypeExpr>) {
        if let Some(t) = t {
            self.ty(t);
        }
    }

    fn ty(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named { name, .. } => {
                if let Some((root, _)) = name.split_once('.') {
                    self.roots.insert(root.to_string());
                }
            }
            TypeExpr::Pointer { to, .. } => self.ty(to),
            TypeExpr::Slice { elem, .. } => self.ty(elem),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.expr(l);
                }
                self.ty(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.ty(key);
                self.ty(value);
            }
            TypeExpr::Chan { elem, .. } => self.ty(elem),
            TypeExpr::Option { elem, .. } => self.ty(elem),
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.ty(&p.ty);
                }
                for r in results {
                    self.ty(r);
                }
            }
        }
    }

    // -- issue #425: operand kind resolution & checks -----------------------

    fn kind_of_ty(&self, ty: &TypeExpr, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        match ty {
            TypeExpr::Named { name, .. } => self.kind_of_named(name, depth),
            TypeExpr::Pointer { to, .. } => {
                OpKind::Pointer(Box::new(self.kind_of_ty(to, depth + 1)))
            }
            TypeExpr::Slice { elem, .. } => {
                OpKind::Slice(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Array { elem, .. } => {
                OpKind::Array(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Map { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, depth + 1)),
                Box::new(self.kind_of_ty(value, depth + 1)),
            ),
            TypeExpr::Chan { elem, .. } => {
                OpKind::Chan(Box::new(self.kind_of_ty(elem, depth + 1)))
            }
            TypeExpr::Option { .. } => OpKind::Unknown,
            TypeExpr::Fn { results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(r, depth + 1))
                    .collect(),
            ),
        }
    }

    fn kind_of_named(&self, name: &str, depth: usize) -> OpKind {
        if depth > 8 {
            return OpKind::Unknown;
        }
        let base = Checker::base_named_type_str(name);
        if self.ck.structs.contains_key(base) {
            return OpKind::Struct(Some(base.to_string()));
        }
        if let Some(target) = self.ck.type_aliases.get(base) {
            return self.kind_of_ty(target, depth + 1);
        }
        match name {
            "bool" => OpKind::Bool,
            "string" => OpKind::Str,
            "float32" | "float64" | "f32" | "f64" => OpKind::Float,
            "int" | "i8" | "int8" | "i16" | "int16" | "i32" | "int32" | "i64" | "int64" | "u8"
            | "uint8" | "u16" | "uint16" | "u32" | "uint32" | "u64" | "uint64" | "usize"
            | "isize" | "uintptr" | "byte" | "rune" | "char" => OpKind::Int,
            _ => OpKind::Unknown,
        }
    }

    fn kind_of_place(&self, p: &Place) -> OpKind {
        match &p.kind {
            PlaceKind::Ident(n) => self.lookup_kind(n).unwrap_or(OpKind::Unknown),
            PlaceKind::Index { base, .. } => match self.kind_of_place(base) {
                OpKind::Slice(e) | OpKind::Array(e) => *e,
                OpKind::Map(_, v) => *v,
                OpKind::Str => OpKind::Int,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => *e,
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            PlaceKind::Field { base, name } => {
                let bk = self.kind_of_place(base);
                self.struct_field_kind(&bk, name).unwrap_or(OpKind::Unknown)
            }
            PlaceKind::AddrOf { base } => OpKind::Pointer(Box::new(self.kind_of_place(base))),
        }
    }

    fn struct_name(&self, k: &OpKind) -> Option<String> {
        match k {
            OpKind::Struct(Some(s)) => Some(s.clone()),
            OpKind::Pointer(inner) => self.struct_name(inner),
            _ => None,
        }
    }

    fn struct_field_kind(&self, k: &OpKind, field: &str) -> Option<OpKind> {
        let sname = self.struct_name(k)?;
        let fields = self.ck.structs.get(&sname)?;
        fields
            .iter()
            .find(|(n, _)| n == field)
            .map(|(_, ty)| self.kind_of_ty(ty, 0))
    }

    fn kind_of_expr(&self, e: &Expr) -> OpKind {
        match e {
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    return k;
                }
                if name == "true" || name == "false" {
                    return OpKind::Bool;
                }
                let head = Checker::base_named_type_str(name);
                if let Some(f) = self.ck.fns.get(name.as_str()).or_else(|| self.ck.fns.get(head)) {
                    return OpKind::Fn(
                        f.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                    );
                }
                OpKind::Unknown
            }
            Expr::Lit { kind, .. } => match kind {
                LitKind::Int(_) | LitKind::Char(_) => OpKind::Int,
                LitKind::Float(_) => OpKind::Float,
                LitKind::Bool(_) => OpKind::Bool,
                LitKind::Str(_) | LitKind::RawStr(_) => OpKind::Str,
                LitKind::Nil => OpKind::Unknown,
            },
            Expr::Interp { .. } => OpKind::Str,
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. } => self.kind_of_expr(inner),
            Expr::AddrOf { operand, .. } => OpKind::Pointer(Box::new(self.kind_of_expr(operand))),
            Expr::Unary { op, operand, .. } => match op {
                UnOp::Addr => OpKind::Pointer(Box::new(self.kind_of_expr(operand))),
                UnOp::Not => OpKind::Bool,
                UnOp::Neg => match self.kind_of_expr(operand) {
                    OpKind::Float => OpKind::Float,
                    OpKind::Int => OpKind::Int,
                    _ => OpKind::Unknown,
                },
            },
            Expr::Binary { op, lhs, rhs, .. } => match op {
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or => OpKind::Bool,
                BinOp::Add => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk == OpKind::Str && rk == OpKind::Str {
                        OpKind::Str
                    } else if lk.is_numeric() && rk.is_numeric() {
                        if lk == OpKind::Float || rk == OpKind::Float {
                            OpKind::Float
                        } else {
                            OpKind::Int
                        }
                    } else {
                        OpKind::Unknown
                    }
                }
                BinOp::Sub | BinOp::Mul | BinOp::Div => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk.is_numeric() && rk.is_numeric() {
                        if lk == OpKind::Float || rk == OpKind::Float {
                            OpKind::Float
                        } else {
                            OpKind::Int
                        }
                    } else {
                        OpKind::Unknown
                    }
                }
                BinOp::Mod
                | BinOp::BitOr
                | BinOp::BitXor
                | BinOp::BitAnd
                | BinOp::Shl
                | BinOp::Shr => {
                    let lk = self.kind_of_expr(lhs);
                    let rk = self.kind_of_expr(rhs);
                    if lk == OpKind::Int && rk == OpKind::Int {
                        OpKind::Int
                    } else {
                        OpKind::Unknown
                    }
                }
            },
            Expr::Receive { chan, .. } => match self.kind_of_expr(chan) {
                OpKind::Chan(elem) => *elem,
                _ => OpKind::Unknown,
            },
            Expr::Index { base, .. } => match self.kind_of_expr(base) {
                OpKind::Slice(e) | OpKind::Array(e) => *e,
                OpKind::Map(_, v) => *v,
                OpKind::Str => OpKind::Int,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => *e,
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            Expr::Slice { base, .. } => match self.kind_of_expr(base) {
                OpKind::Slice(e) | OpKind::Array(e) => OpKind::Slice(e),
                OpKind::Str => OpKind::Str,
                OpKind::Pointer(inner) => match *inner {
                    OpKind::Array(e) => OpKind::Slice(e),
                    _ => OpKind::Unknown,
                },
                _ => OpKind::Unknown,
            },
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                self.struct_field_kind(&bk, name).unwrap_or(OpKind::Unknown)
            }
            Expr::StructLit { ty, .. } => ty
                .as_ref()
                .map(|t| self.kind_of_ty(t, 0))
                .unwrap_or(OpKind::Struct(None)),
            Expr::SliceLit { elem, items, .. } => {
                let ek = elem
                    .as_ref()
                    .map(|t| self.kind_of_ty(t, 0))
                    .or_else(|| items.first().map(|i| self.kind_of_expr(i)))
                    .unwrap_or(OpKind::Unknown);
                OpKind::Slice(Box::new(ek))
            }
            Expr::MapLit { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, 0)),
                Box::new(self.kind_of_ty(value, 0)),
            ),
            Expr::FnLit { results, .. } => OpKind::Fn(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(&r.ty, 0))
                    .collect(),
            ),
            Expr::Call { callee, args, .. } => match self.call_result_kinds(callee, args) {
                Some(res) if res.is_empty() => OpKind::Void,
                Some(res) => res[0].clone(),
                None => OpKind::Unknown,
            },
        }
    }

    fn call_result_kinds(&self, callee: &Expr, args: &[CallArg]) -> Option<Vec<OpKind>> {
        match unwrap_parens(callee) {
            Expr::FnLit { results, .. } => Some(
                results
                    .iter()
                    .map(|r| self.kind_of_ty(&r.ty, 0))
                    .collect(),
            ),
            Expr::SliceLit {
                elem: Some(t),
                items,
                ..
            } if items.is_empty() => Some(vec![OpKind::Slice(Box::new(self.kind_of_ty(t, 0)))]),
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    if let OpKind::Fn(res) = k {
                        return Some(res);
                    }
                    return None;
                }
                match name.as_str() {
                    "len" | "cap" => return Some(vec![OpKind::Int]),
                    "append" => {
                        let first = args
                            .first()
                            .map(|a| self.kind_of_expr(&a.value))
                            .unwrap_or_else(|| OpKind::Slice(Box::new(OpKind::Unknown)));
                        return Some(vec![first]);
                    }
                    "make" => return Some(vec![self.make_kind(args)]),
                    "new" => return Some(vec![OpKind::Pointer(Box::new(OpKind::Unknown))]),
                    "delete" | "close" => return Some(Vec::new()),
                    _ => {}
                }
                let conv = self.kind_of_named(name, 0);
                if !matches!(conv, OpKind::Unknown | OpKind::Struct(_)) {
                    return Some(vec![conv]);
                }
                let head = Checker::base_named_type_str(name);
                if let Some(f) = self.ck.fns.get(name.as_str()).or_else(|| self.ck.fns.get(head)) {
                    return Some(
                        f.results
                            .iter()
                            .map(|r| self.kind_of_ty(&r.ty, 0))
                            .collect(),
                    );
                }
                None
            }
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                let head = Checker::base_named_type_str(name);
                if let Some(sname) = self.struct_name(&bk) {
                    if let Some(m) = self.ck.method_decls.get(&(sname, head.to_string())) {
                        return Some(
                            m.results
                                .iter()
                                .map(|r| self.kind_of_ty(&r.ty, 0))
                                .collect(),
                        );
                    }
                }
                if let Some(OpKind::Fn(res)) = self.struct_field_kind(&bk, name) {
                    return Some(res);
                }
                None
            }
            _ => None,
        }
    }

    fn make_kind(&self, args: &[CallArg]) -> OpKind {
        let Some(first) = args.first() else {
            return OpKind::Unknown;
        };
        match unwrap_parens(&first.value) {
            Expr::StructLit { ty: Some(t), .. } => self.kind_of_ty(t, 0),
            Expr::SliceLit { elem: Some(t), .. } => OpKind::Slice(Box::new(self.kind_of_ty(t, 0))),
            Expr::MapLit { key, value, .. } => OpKind::Map(
                Box::new(self.kind_of_ty(key, 0)),
                Box::new(self.kind_of_ty(value, 0)),
            ),
            Expr::Ident { name, .. } => self.kind_of_named(name, 0),
            _ => OpKind::Unknown,
        }
    }

    fn emit_e1002(&mut self, span: Span, msg: String, note: &str) {
        self.diags.push(
            Diagnostic::error("HYB-E1002", self.path.clone(), span, msg)
                .with_notes(vec![note.to_string()]),
        );
    }

    fn emit_e2005(&mut self, span: Span, msg: String, note: &str) {
        self.diags.push(
            Diagnostic::error("HYB-E2005", self.path.clone(), span, msg)
                .with_notes(vec![note.to_string()]),
        );
    }

    /// Family A (#425): `if cond` and `for cond` require a boolean condition.
    fn check_bool_condition(&mut self, cond: &Expr, construct: &str) {
        let k = self.kind_of_expr(cond);
        if !matches!(k, OpKind::Unknown | OpKind::Bool) {
            self.emit_e1002(
                cond.span(),
                format!(
                    "non-boolean condition `{}` (type `{}`) in `{}` statement — Go requires a `bool` condition (HYB-E1002)",
                    cond.render(),
                    k.label(),
                    construct
                ),
                "compare the value explicitly (e.g. `x != 0`, `len(s) > 0`) or pass a `bool` expression",
            );
        }
    }

    /// Family B1/B2 (#425): unary `!` requires `bool`, unary `-` requires a numeric operand.
    fn check_unary_operand(&mut self, op: UnOp, operand: &Expr, span: Span) {
        let k = self.kind_of_expr(operand);
        match op {
            UnOp::Not => {
                if !matches!(k, OpKind::Unknown | OpKind::Bool) {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `!` not defined on `{}` (type `{}`) — `!` requires a `bool` operand (HYB-E1002)",
                            operand.render(),
                            k.label()
                        ),
                        "compare numeric or string values explicitly (e.g. `x == 0`, `s == \"\"`)",
                    );
                }
            }
            UnOp::Neg => {
                if !matches!(k, OpKind::Unknown | OpKind::Int | OpKind::Float) {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `-` not defined on `{}` (type `{}`) — unary `-` requires a numeric operand (HYB-E1002)",
                            operand.render(),
                            k.label()
                        ),
                        "unary `-` is only defined on integer and floating-point values",
                    );
                }
            }
            UnOp::Addr => {}
        }
    }

    /// Family B3/B4/B5 (#425): binary operator operand-kind checks.
    fn check_binary_operands(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr, span: Span) {
        let lk = self.kind_of_expr(lhs);
        let rk = self.kind_of_expr(rhs);
        match op {
            // `==` / `!=` are checked by `emit_guard.rs` (HYB-E2006).
            BinOp::Eq | BinOp::Ne => {}
            // B4: `&&` and `||` require `bool` operands.
            BinOp::And | BinOp::Or => {
                let op_str = if matches!(op, BinOp::And) { "&&" } else { "||" };
                if !matches!(lk, OpKind::Unknown | OpKind::Bool)
                    || !matches!(rk, OpKind::Unknown | OpKind::Bool)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` requires `bool` operands, found `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "both sides of `&&` and `||` must evaluate to `bool`",
                    );
                }
            }
            // B5: ordered comparisons `<`, `<=`, `>`, `>=` are defined only on numeric or string operands.
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                let op_str = match op {
                    BinOp::Lt => "<",
                    BinOp::Le => "<=",
                    BinOp::Gt => ">",
                    _ => ">=",
                };
                let l_ok = matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let r_ok = matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = lk != OpKind::Unknown
                    && rk != OpKind::Unknown
                    && lk.is_numeric() != rk.is_numeric();
                if !l_ok || !r_ok || mismatch {
                    self.emit_e1002(
                        span,
                        format!(
                            "operator `{}` not defined on `{}` and `{}` in `{}` (HYB-E1002)",
                            op_str,
                            lk.label(),
                            rk.label(),
                            span_expr_label(lhs, op_str, rhs)
                        ),
                        "ordered comparisons (`<`, `<=`, `>`, `>=`) require two numeric values or two strings",
                    );
                }
            }
            // B3: `+` accepts numeric+numeric or string+string.
            BinOp::Add => {
                let l_ok = matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let r_ok = matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = lk != OpKind::Unknown
                    && rk != OpKind::Unknown
                    && lk.is_numeric() != rk.is_numeric();
                if !l_ok || !r_ok || mismatch {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}` (mismatched or non-addable types `{}` and `{}`) (HYB-E1002)",
                            span_expr_label(lhs, "+", rhs),
                            lk.label(),
                            rk.label()
                        ),
                        "`+` requires two numeric operands or two strings; use string interpolation `\"{x}{s}\"` to format mixed values",
                    );
                }
            }
            // `-`, `*`, `/` require numeric operands.
            BinOp::Sub | BinOp::Mul | BinOp::Div => {
                let op_str = match op {
                    BinOp::Sub => "-",
                    BinOp::Mul => "*",
                    _ => "/",
                };
                if !matches!(lk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` not defined on `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "arithmetic `-`, `*`, `/` require numeric operands",
                    );
                }
            }
            // `%`, bitwise, and shifts require integer operands.
            BinOp::Mod
            | BinOp::BitOr
            | BinOp::BitXor
            | BinOp::BitAnd
            | BinOp::Shl
            | BinOp::Shr => {
                let op_str = match op {
                    BinOp::Mod => "%",
                    BinOp::BitOr => "|",
                    BinOp::BitXor => "^",
                    BinOp::BitAnd => "&",
                    BinOp::Shl => "<<",
                    _ => ">>",
                };
                if !matches!(lk, OpKind::Unknown | OpKind::Int)
                    || !matches!(rk, OpKind::Unknown | OpKind::Int)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}`: operator `{}` requires integer operands, found `{}` and `{}` (HYB-E1002)",
                            span_expr_label(lhs, op_str, rhs),
                            op_str,
                            lk.label(),
                            rk.label()
                        ),
                        "modulo, bitwise, and shift operators require integer operands",
                    );
                }
            }
        }
    }

    /// Family B6 (#425): `s++`, `s--`, and compound assignment operand checks.
    fn check_assign_operands(
        &mut self,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
        span: Span,
    ) {
        let Some(target) = targets.first() else {
            return;
        };
        let tk = self.kind_of_place(target);
        match op {
            AssignOp::Assign => {}
            AssignOp::Inc | AssignOp::Dec => {
                let op_str = if matches!(op, AssignOp::Inc) { "++" } else { "--" };
                if !matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float) {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{}{}` (non-numeric type `{}`) (HYB-E1002)",
                            target.render(),
                            op_str,
                            tk.label()
                        ),
                        "`++` and `--` require a numeric variable or place",
                    );
                }
            }
            AssignOp::AddAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                let t_ok = matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let v_ok = matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float | OpKind::Str);
                let mismatch = tk != OpKind::Unknown
                    && vk != OpKind::Unknown
                    && tk.is_numeric() != vk.is_numeric();
                if !t_ok || !v_ok || mismatch {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid operation `{} += {}` (mismatched or non-addable types `{}` and `{}`) (HYB-E1002)",
                            target.render(),
                            val.render(),
                            tk.label(),
                            vk.label()
                        ),
                        "`+=` requires two numeric values or two strings",
                    );
                }
            }
            AssignOp::SubAssign | AssignOp::MulAssign | AssignOp::DivAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                if !matches!(tk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int | OpKind::Float)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid compound assignment on `{}` (type `{}`) and `{}` (type `{}`) (HYB-E1002)",
                            target.render(),
                            tk.label(),
                            val.render(),
                            vk.label()
                        ),
                        "`-=`, `*=`, `/=` require numeric operands",
                    );
                }
            }
            AssignOp::ModAssign
            | AssignOp::BitOrAssign
            | AssignOp::BitAndAssign
            | AssignOp::BitXorAssign
            | AssignOp::ShlAssign
            | AssignOp::ShrAssign => {
                let Some(val) = values.first() else { return };
                let vk = self.kind_of_expr(val);
                if !matches!(tk, OpKind::Unknown | OpKind::Int)
                    || !matches!(vk, OpKind::Unknown | OpKind::Int)
                {
                    self.emit_e1002(
                        span,
                        format!(
                            "invalid integer compound assignment on `{}` (type `{}`) and `{}` (type `{}`) (HYB-E1002)",
                            target.render(),
                            tk.label(),
                            val.render(),
                            vk.label()
                        ),
                        "integer compound assignment operators require integer operands",
                    );
                }
            }
        }
    }

    /// Family C1 (#425): `base[index]` requires a slice, array, map, or string.
    fn check_index_kind(
        &mut self,
        base_str: &str,
        bk: &OpKind,
        index: &Expr,
        ik: &OpKind,
        span: Span,
    ) {
        let indexable = matches!(
            bk,
            OpKind::Unknown
                | OpKind::Slice(_)
                | OpKind::Array(_)
                | OpKind::Map(_, _)
                | OpKind::Str
        ) || matches!(
            bk,
            OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
        );
        if !indexable {
            self.emit_e1002(
                span,
                format!(
                    "cannot index `{}` (type `{}`) — indexing requires a slice, array, map, or string (HYB-E1002)",
                    base_str,
                    bk.label()
                ),
                "only slices, arrays, maps, and strings can be indexed with `x[i]`",
            );
            return;
        }
        // For slices, arrays, and strings, the index must be an integer.
        let needs_int_idx = matches!(bk, OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str)
            || matches!(bk, OpKind::Pointer(inner) if matches!(**inner, OpKind::Array(_)));
        if needs_int_idx && !matches!(ik, OpKind::Unknown | OpKind::Int) {
            self.emit_e1002(
                index.span(),
                format!(
                    "non-integer index `{}` (type `{}`) on `{}` (HYB-E1002)",
                    index.render(),
                    ik.label(),
                    bk.label()
                ),
                "slice, array, and string indices must be integers",
            );
        }
    }

    /// Family C2 (#425): `base[lo:hi]` requires a slice, array, or string.
    fn check_slice_kind(
        &mut self,
        base: &Expr,
        bk: &OpKind,
        lo: Option<&Expr>,
        hi: Option<&Expr>,
        span: Span,
    ) {
        let sliceable = matches!(
            bk,
            OpKind::Unknown | OpKind::Slice(_) | OpKind::Array(_) | OpKind::Str
        ) || matches!(
            bk,
            OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
        );
        if !sliceable {
            self.emit_e1002(
                span,
                format!(
                    "cannot slice `{}` (type `{}`) — slicing requires a slice, array, or string (HYB-E1002)",
                    base.render(),
                    bk.label()
                ),
                "only slices, arrays, and strings support `x[lo:hi]` slicing",
            );
            return;
        }
        for bound in [lo, hi].into_iter().flatten() {
            let k = self.kind_of_expr(bound);
            if !matches!(k, OpKind::Unknown | OpKind::Int) {
                self.emit_e1002(
                    bound.span(),
                    format!(
                        "non-integer slice bound `{}` (type `{}`) (HYB-E1002)",
                        bound.render(),
                        k.label()
                    ),
                    "slice bounds must be integers",
                );
            }
        }
    }

    /// Family D1 & Family E (#425): non-function call check + binding-shaped builtin arg checks.
    fn check_call_operands(&mut self, callee: &Expr, args: &[CallArg], span: Span) {
        let bare = unwrap_parens(callee);
        match bare {
            // `[]byte("ok")` / `[]rune(s)`: parser encodes slice type conversions as
            // `Call { callee: SliceLit { elem: Some(T), items: [] }, args: [x] }`.
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() => {}
            Expr::Ident { name, .. } => {
                if let Some(k) = self.lookup_kind(name) {
                    if !matches!(k, OpKind::Unknown | OpKind::Fn(_)) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot call `{}` (variable of type `{}`): `{}` is not a function (HYB-E1002)",
                                name,
                                k.label(),
                                k.label()
                            ),
                            "only functions, methods, closures, and type conversions can be called",
                        );
                    }
                } else {
                    self.check_builtin_operand_kinds(name, args, span);
                }
            }
            Expr::Field { base, name, .. } => {
                let bk = self.kind_of_expr(base);
                if let Some(fk) = self.struct_field_kind(&bk, name) {
                    let has_method = self
                        .struct_name(&bk)
                        .is_some_and(|sn| self.ck.method_decls.contains_key(&(sn, name.clone())));
                    if !has_method && !matches!(fk, OpKind::Unknown | OpKind::Fn(_)) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot call `{}` (field of type `{}`): `{}` is not a function (HYB-E1002)",
                                callee.render(),
                                fk.label(),
                                fk.label()
                            ),
                            "only function-typed struct fields or declared methods can be called",
                        );
                    }
                }
            }
            _ => {
                let ck = self.kind_of_expr(bare);
                if !matches!(ck, OpKind::Unknown | OpKind::Fn(_)) {
                    self.emit_e1002(
                        span,
                        format!(
                            "cannot call `{}` (type `{}`): `{}` is not a function (HYB-E1002)",
                            callee.render(),
                            ck.label(),
                            ck.label()
                        ),
                        "only functions, methods, closures, and type conversions can be called",
                    );
                }
            }
        }
    }

    /// Family E1–E5 (#425): extend `HYB-E2005` (`BUILTIN-ARGS`) from literals
    /// (`builtins.rs`) to resolved binding shapes.
    fn check_builtin_operand_kinds(&mut self, name: &str, args: &[CallArg], span: Span) {
        match name {
            "len" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_len_cap(op, false) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                let ok = matches!(
                    ak,
                    OpKind::Unknown
                        | OpKind::Str
                        | OpKind::Slice(_)
                        | OpKind::Array(_)
                        | OpKind::Map(_, _)
                        | OpKind::Chan(_)
                ) || matches!(
                    &ak,
                    OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
                );
                if !ok {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `len` (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`len` accepts a string, slice, array, map, or channel",
                    );
                }
            }
            "cap" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_len_cap(op, true) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                let ok = matches!(
                    ak,
                    OpKind::Unknown | OpKind::Slice(_) | OpKind::Array(_) | OpKind::Chan(_)
                ) || matches!(
                    &ak,
                    OpKind::Pointer(inner) if matches!(**inner, OpKind::Unknown | OpKind::Array(_))
                );
                if !ok {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid argument `{}` (type `{}`) for built-in `cap` (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`cap` accepts a slice, array, or channel — maps and strings have no capacity",
                    );
                }
            }
            "close" => {
                if args.len() != 1 {
                    return; // arity handled by builtins.rs
                }
                let op = &args[0].value;
                if builtins_already_flags_close(op) {
                    return;
                }
                let ak = self.kind_of_expr(op);
                if !matches!(ak, OpKind::Unknown | OpKind::Chan(_)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "cannot close non-channel `{}` (type `{}`) (HYB-E2005)",
                            op.render(),
                            ak.label()
                        ),
                        "`close` requires a bidirectional or send-only channel (`chan T` or `chan<- T`)",
                    );
                }
            }
            "delete" => {
                if args.len() != 2 {
                    return; // arity handled by builtins.rs
                }
                let map_arg = &args[0].value;
                let key_arg = &args[1].value;
                let mk = self.kind_of_expr(map_arg);
                match &mk {
                    OpKind::Unknown => {}
                    OpKind::Map(k_expected, _) => {
                        let kk = self.kind_of_expr(key_arg);
                        if **k_expected != OpKind::Unknown
                            && kk != OpKind::Unknown
                            && kk != **k_expected
                        {
                            self.emit_e2005(
                                span,
                                format!(
                                    "cannot use `{}` (type `{}`) as key type `{}` in `delete` (HYB-E2005)",
                                    key_arg.render(),
                                    kk.label(),
                                    k_expected.label()
                                ),
                                "the second argument to `delete(m, k)` must match the map's key type",
                            );
                        }
                    }
                    _ => {
                        self.emit_e2005(
                            span,
                            format!(
                                "invalid argument `{}` (type `{}`) is not a map in `delete` (HYB-E2005)",
                                map_arg.render(),
                                mk.label()
                            ),
                            "`delete(m, k)` requires a map as its first argument",
                        );
                    }
                }
            }
            "append" => {
                if args.is_empty() {
                    self.emit_e2005(
                        span,
                        "`append` requires at least 1 argument (slice[, elems...]) (HYB-E2005)"
                            .to_string(),
                        "pass a slice as the first argument to `append(xs, elem)`",
                    );
                    return;
                }
                let slice_arg = &args[0].value;
                let sk = self.kind_of_expr(slice_arg);
                if !matches!(sk, OpKind::Unknown | OpKind::Slice(_)) {
                    self.emit_e2005(
                        span,
                        format!(
                            "invalid `append`: first argument `{}` (type `{}`) must be a slice (HYB-E2005)",
                            slice_arg.render(),
                            sk.label()
                        ),
                        "the first argument to `append(xs, ...)` must be a slice (`[]T`)",
                    );
                }
            }
            _ => {}
        }
    }

    // -- results -------------------------------------------------------------

    /// Form 2: a declared Go import whose root is never mentioned anywhere
    /// in the file is carried into the emitted header and never builds.
    /// `rust/` imports are skipped — the emitter never writes them.
    fn check_imports(&mut self, file: &File) {
        for d in &file.decls {
            let Decl::Import { path, span } = d else {
                continue;
            };
            if path.starts_with("rust/") {
                continue;
            }
            let root = path.rsplit('/').next().unwrap_or(path.as_str());
            if !self.roots.contains(root) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        *span,
                        format!(
                            "import `{}` is never used — the emitted Go carries it into the header and Go refuses `imported and not used` (HYB-E1002)",
                            root
                        ),
                    )
                    .with_notes(vec![
                        "drop the import, or use it — any mention of the package root counts"
                            .to_string(),
                    ]),
                );
            }
        }
    }

    /// All fn-local declarations have the same Go unused-binding rule.
    fn finish(&mut self) {
        for d in &self.defines {
            if !d.used && (!d.elidable || d.written) {
                let emitted = match d.kind {
                    LocalKind::Define | LocalKind::Let => format!("{} := ...", d.name),
                    LocalKind::Var | LocalKind::Const => format!("var {}", d.name),
                };
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        d.span,
                        format!(
                            "`{}` is declared and never used — Go refuses to build the emitted `{}` (declared and not used) (HYB-E1002)",
                            d.name, emitted
                        ),
                    )
                    .with_notes(vec![format!(
                        "a read anywhere in this function counts, including inside a closure; `_ = {}` also counts as a use",
                        d.name
                    )]),
                );
            }
        }
    }
}

fn span_expr_label(lhs: &Expr, op: &str, rhs: &Expr) -> String {
    format!("{} {} {}", lhs.render(), op, rhs.render())
}

fn builtins_already_flags_close(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Lit { .. } => true,
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => matches!(unwrap_parens(operand), Expr::Lit { .. }),
        _ => false,
    }
}

fn builtins_already_flags_len_cap(e: &Expr, is_cap: bool) -> bool {
    let bare = unwrap_parens(e);
    let num_bool_char = match bare {
        Expr::Lit { kind, .. } => matches!(
            kind,
            LitKind::Int(_) | LitKind::Float(_) | LitKind::Bool(_) | LitKind::Char(_)
        ),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => matches!(
            unwrap_parens(operand),
            Expr::Lit {
                kind: LitKind::Int(_) | LitKind::Float(_),
                ..
            }
        ),
        _ => false,
    };
    if num_bool_char {
        return true;
    }
    is_cap
        && matches!(
            bare,
            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            }
        )
}

#[cfg(test)]
mod operand_kind_tests {
    use crate::{check_file, Verdict};
    use hy_diag::Severity;
    use hy_syntax::parse_file;

    fn check(src: &str) -> crate::CheckResult {
        let ast = parse_file("test.hy", src).expect("test snippet must parse");
        check_file(&ast)
    }

    fn error_codes(r: &crate::CheckResult) -> Vec<String> {
        r.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| d.code.clone())
            .collect()
    }

    #[test]
    fn rejects_non_boolean_conditions_a1_a2() {
        for src in [
            "fn main() {\n    x := 5\n    if x { fmt.println(1) }\n}\n",
            "fn main() {\n    x := 5\n    for x { x = 0 }\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_operator_operand_mismatches_b1_to_b6() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(!x)\n}\n",
            "fn main() {\n    s := \"a\"\n    fmt.println(-s)\n}\n",
            "fn main() {\n    x := 5\n    s := \"a\"\n    fmt.println(x + s)\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x && true)\n}\n",
            "fn main() {\n    b := true\n    fmt.println(b < false)\n}\n",
            "fn main() {\n    s := \"a\"\n    s++\n    fmt.println(s)\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_non_indexable_and_non_callable_c1_c2_d1() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(x[0])\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x[0:1])\n}\n",
            "fn main() {\n    x := 5\n    fmt.println(x())\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E1002".to_string()),
                "expected HYB-E1002, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn rejects_binding_shaped_builtin_misuse_e1_to_e5() {
        for src in [
            "fn main() {\n    x := 5\n    fmt.println(len(x))\n}\n",
            "fn main() {\n    x := 5\n    close(x)\n}\n",
            "fn main() {\n    m := map[string]int{}\n    fmt.println(cap(m))\n}\n",
            "fn main() {\n    xs := []int{1}\n    delete(xs, 0)\n    fmt.println(xs)\n}\n",
            "fn main() {\n    x := 5\n    x = append(x, 1)\n    fmt.println(x)\n}\n",
        ] {
            let r = check(src);
            assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
            assert!(
                error_codes(&r).contains(&"HYB-E2005".to_string()),
                "expected HYB-E2005, got {:?} for:\n{src}",
                error_codes(&r)
            );
        }
    }

    #[test]
    fn accepts_valid_operand_and_builtin_controls() {
        let src = "\
fn main() {
    ok := true
    if ok {
        fmt.println(!ok)
    }
    n := 5
    fmt.println(-n)
    fmt.println(n + 2)
    s := \"hello\"
    fmt.println(s + \" world\")
    fmt.println(s[0:2])
    fmt.println(len(s))
    xs := []int{1, 2}
    xs = append(xs, 3)
    fmt.println(xs[0])
    fmt.println(cap(xs))
    m := map[string]int{\"k\": 1}
    delete(m, \"k\")
    fmt.println(len(m))
    ch := make(chan int, 1)
    close(ch)
    f := fn() int { return 7 }
    fmt.println(f())
}
";
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "unexpected diagnostics: {:?}",
            r.diagnostics
        );
    }
}
