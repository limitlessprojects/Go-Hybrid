//! buildable.rs — issue #134: an accepted program must emit buildable Go.
//!
//! Go's front-end rejects three declaration shapes that `hyb check` used to
//! accept with 0 diagnostics, so the emitted module could never build:
//!
//!   1. an unused `:=` binding — `x := 5` with no read of `x` is
//!      `declared and not used`; `_ := 5` (and `_, _ := ...`) declares no
//!      new variable at all (`no new variables on left side of :=`);
//!   2. an unused import — the emitter carries every declared Go import
//!      into the header, and Go rejects `imported and not used`;
//!   3. a repeated name on the left of one `:=` — `x, x := 1, 2` is
//!      `x repeated on left side of :=`.
//!
//! All three reject fail-closed as HYB-E1002 (registry rule `BUILDABLE`):
//! what the checker accepts must survive `go build`.
//!
//! The read-set is deliberately Go-faithful, not conservative-in-the-wrong-
//! direction: a name is "read" when Go's own unused-variable check counts it
//! — any mention in the function, including inside closure bodies, and
//! including `x++`, `x += 1`, `x[k] = v`, `x.f = v` and `&x` — while a bare
//! `x = v` store is NOT a read (gc reports `declared and not used` for a
//! write-only local). The same-scope re-define (`x, y := 1, 2` after
//! `x := 0`) keeps the existing variable, exactly as Go's `:=` does.
//!
//! Out of scope, documented: unused `var`/composite-`const` locals,
//! select-receive names and match bindings (the same Go error family, but
//! not the issue's three forms), unread `for ... in` bindings (lowered to
//! `_` at emission, issue #139), and unused `rust/` imports (they are never
//! emitted as Go imports, so they cannot break the build).

use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Block, Decl, Expr, File, FnDecl, ForKind, InterpPart, LocalDecl, LocalKind,
    MatchPattern, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};
use std::collections::{HashMap, HashSet};

use crate::model::Checker;

/// One `:=`/`let` name that must be read somewhere in its function.
struct Define {
    name: String,
    span: Span,
    used: bool,
}

#[derive(Default)]
struct Collector {
    /// One layer per open block/closure. `Some(id)` = a checked `:=`/`let`
    /// define; `None` = a name that shadows (params, receivers, `var`,
    /// `for`/select/match bindings) but is outside the issue's scope.
    scopes: Vec<HashMap<String, Option<usize>>>,
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
    let mut c = Collector {
        path: ck.path.clone(),
        ..Collector::default()
    };
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
    ck.diags.extend(c.diags);
}

impl Collector {
    // -- scope bookkeeping ---------------------------------------------------

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// A name that shadows checked defines but is not itself checked.
    fn exempt(&mut self, name: &str) {
        if name != "_" {
            if let Some(top) = self.scopes.last_mut() {
                top.insert(name.to_string(), None);
            }
        }
    }

    /// Every identifier mention feeds the import-usage set.
    fn mention(&mut self, name: &str) {
        self.roots.insert(name.to_string());
    }

    /// A read of `name`: marks the innermost checked define it resolves to.
    /// A read inside a closure resolves through the closure's scope layers
    /// to the captured outer define — exactly what Go's check counts.
    fn read(&mut self, name: &str) {
        self.mention(name);
        for layer in self.scopes.iter().rev() {
            if let Some(entry) = layer.get(name) {
                if let Some(id) = entry {
                    self.defines[*id].used = true;
                }
                return;
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
        }
        for p in &f.params {
            self.exempt(&p.name);
        }
        self.block_stmts(body);
        self.pop_scope();
    }

    fn local(&mut self, l: &LocalDecl) {
        let checked = matches!(l.kind, LocalKind::Define | LocalKind::Let);
        if checked {
            // Form 3: `x, x := 1, 2` — `x repeated on left side of :=`.
            let mut seen: HashSet<&str> = HashSet::new();
            for name in &l.names {
                if name != "_" && !seen.insert(name.as_str()) {
                    self.diags.push(
                        Diagnostic::error(
                            "HYB-E1002",
                            self.path.clone(),
                            l.span,
                            format!(
                                "`{}` is repeated on the left of one `:=` — Go refuses `{} := ...` (repeated on left side of :=) (HYB-E1002)",
                                name,
                                l.names.join(", ")
                            ),
                        )
                        .with_notes(vec![
                            "each left-hand name must appear once; write `x, y := 1, 2`"
                                .to_string(),
                        ]),
                    );
                }
            }
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
        for name in &l.names {
            if !checked {
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
                used: false,
            });
            top.insert(name.clone(), Some(id));
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
                ..
            } => {
                for t in targets {
                    self.place(t, *op);
                }
                for v in values {
                    self.expr(v);
                }
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
                self.block_scoped(then);
                if let Some(o) = otherwise {
                    self.stmt(o);
                }
                self.pop_scope();
            }
            Stmt::For { kind, body, .. } => {
                self.push_scope();
                match kind {
                    ForKind::While { cond } => self.expr(cond),
                    ForKind::Range { var, lo, hi, .. } => {
                        self.expr(lo);
                        self.expr(hi);
                        // The loop head always reads the counter.
                        self.exempt(var);
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.expr(expr);
                        // Unread iterate bindings are lowered to `_` at
                        // emission (issue #139) — outside this pass.
                        for v in vars {
                            self.exempt(v);
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
                            self.exempt(b);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.expr(g);
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
                        SelectArmKind::Send { chan, value } => {
                            self.expr(chan);
                            self.expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.expr(chan);
                            // Go does check select-receive names; outside
                            // the issue's three forms (documented).
                            for n in names {
                                self.exempt(n);
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

    // -- places (assignment targets) ----------------------------------------

    fn place(&mut self, p: &Place, op: AssignOp) {
        match &p.kind {
            PlaceKind::Ident(n) => {
                // A bare `x = v` store is the ONE target form Go does not
                // count as a read; `x++`, `x += 1` and every non-ident
                // target (`x[k] = v`, `x.f = v`, `&x`) read the root.
                if matches!(op, AssignOp::Assign) {
                    self.mention(n);
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
            Expr::Binary { lhs, rhs, .. } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            Expr::Unary { operand, .. } => self.expr(operand),
            Expr::AddrOf { operand, .. } => self.expr(operand),
            Expr::Move { operand, .. } => self.expr(operand),
            Expr::Try { operand, .. } => self.expr(operand),
            Expr::Receive { chan, .. } => self.expr(chan),
            Expr::Index { base, index, .. } => {
                self.expr(base);
                self.expr(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.expr(base);
                if let Some(x) = lo {
                    self.expr(x);
                }
                if let Some(x) = hi {
                    self.expr(x);
                }
            }
            Expr::Field { base, .. } => self.expr(base),
            Expr::Call { callee, args, .. } => {
                self.expr(callee);
                for a in args {
                    self.expr(&a.value);
                }
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

    /// Form 1: every checked define must be read somewhere in its function.
    fn finish(&mut self) {
        for d in &self.defines {
            if !d.used {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        d.span,
                        format!(
                            "`{}` is declared and never used — Go refuses to build the emitted `{} := ...` (declared and not used) (HYB-E1002)",
                            d.name, d.name
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
