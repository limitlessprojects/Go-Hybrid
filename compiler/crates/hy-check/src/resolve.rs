//! resolve.rs — issue #228 family A: `HYB-E1001 unresolved-name`.
//!
//! The registry has carried HYB-E1001 since the diagnostic was drafted, but
//! nothing ever constructed it: an identifier that resolves to no local,
//! parameter, file-level declaration, import, or builtin passed `hyb check`
//! with 0 diagnostics and then failed the emitted Go with `undefined: X`
//! (`undefined_fn()`, `y := undefined_var + 1`, `undefined_var = 3`,
//! a closure body reading a name nothing declares, a `[N]T` array length
//! naming a constant that does not exist — issue #228 family A).
//!
//! Dotted first segments are deliberately EXEMPT (see the Field arm): a
//! direct ident base under a `.` is the package-prefix surface, and the
//! emitter's contract lets unknown roots emit verbatim and collect an import
//! (`hex.encode` in spike-10 is exactly that — a root the checker's own
//! foreign table never listed). The checker cannot tell a real root from a
//! typo, so those stay with the go toolchain.
//!
//! The pass is deliberately OVER-APPROXIMATING about scope: the allowed set
//! for a function is every name that is declared ANYWHERE in that function
//! body (locals, params, receivers, loop/select/match/closure bindings,
//! named results, type parameters) plus the file-level tables and the
//! predeclared set. That makes false positives structurally impossible for
//! ordering/shadowing shapes the checker already accepts (`if x := f(); x > 0`
//! where the init's define is only visible to later statements, use before
//! an in-body define, a name defined only inside a closure) while still
//! rejecting exactly the class that breaks the build: a name that no
//! declaration in the file provides at all. Type positions
//! (`TypeExpr::Named`) are out of scope here — foreign/qualified type
//! spellings resolve through their own tables and are a separate family.

use std::collections::HashSet;

use hy_diag::Span;
use hy_syntax::{
    Block, Decl, Expr, FnDecl, InterpPart, MatchPattern, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr,
};

use crate::model::Checker;

/// Go's predeclared identifiers that are legal in VALUE position without any
/// declaration: builtin functions, predeclared types, and the literals that
/// parse as `Lit` but may appear as idents in defensive tables. The SPEC
/// spellings the emitter translates (`u8`, `i8`, …) belong here for the same
/// reason the conversion pass treats `u8(300)` as legal — issue #147.
/// `usize`/`isize` are included conservatively: the emitter deliberately does
/// NOT translate them (SPEC §402 — "there is no usize/isize at the language
/// level") so a value-position use keeps its honest `undefined: usize` go
/// build failure instead of gaining a new check-time claim.
const PREDECLARED: &[&str] = &[
    // Go builtin functions
    "append", "cap", "clear", "close", "complex", "copy", "delete", "imag", "len", "make", "max",
    "min", "new", "panic", "print", "println", "real", "recover",
    // Go predeclared types (also legal as conversions: `int(x)`, `string(b)`)
    "bool", "byte", "complex64", "complex128", "error", "float32", "float64", "int", "int8",
    "int16", "int32", "int64", "rune", "string", "uint", "uint8", "uint16", "uint32", "uint64",
    "uintptr", "any", "comparable",
    // SPEC spellings (emitter-translated conversions, issue #147) + the two
    // SPEC says do not exist at the language level (status quo preserved).
    "u8", "u16", "u32", "u64", "i8", "i16", "i32", "i64", "f32", "f64", "usize", "isize",
    // option surface (example 07: `return some(v)` / `return none`)
    "some", "none",
    // wildcard binding target (`_ = x` is a Place::Ident, not a Lit)
    "_",
];

/// Which names the current walk records.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Record every name this function declares (defines pass).
    Collect,
    /// Report every value-position ident outside the allowed set (check pass).
    Check,
}

struct Scan<'a> {
    path: &'a str,
    packages: &'a HashSet<String>,
    allowed: &'a mut HashSet<String>,
    diags: &'a mut Vec<hy_diag::Diagnostic>,
    mode: Mode,
}

impl<'a> Scan<'a> {
    fn define(&mut self, name: &str) {
        if self.mode == Mode::Collect {
            self.allowed.insert(name.to_string());
        }
    }

    fn ident(&mut self, name: &str, span: Span) {
        if self.mode != Mode::Check {
            return;
        }
        if self.allowed.contains(name) || self.packages.contains(name) {
            return;
        }
        if crate::foreign::BUILTIN_PACKAGES.contains(&name) {
            return;
        }
        let msg = format!(
            "`{}` is not available here — nothing declares this name in the file or this scope \
             (HYB-E1001)",
            name
        );
        self.diags.push(
            hy_diag::Diagnostic::error("HYB-E1001", self.path.to_string(), span, msg).with_notes(
                vec![
                    format!(
                        "declare `{}` before use: a local (`{} := ...`), a parameter, a file-level \
                         `const`/`fn`, or a struct/enum/alias declaration",
                        name, name
                    ),
                    "or `import` the package that provides it — imports (plus the builtin package \
                     table) are the only package-qualified roots"
                        .to_string(),
                    "check the spelling (foreign names normalize to snake_case at binding time)"
                        .to_string(),
                ],
            ),
        );
    }

    fn run_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.run_stmt(s);
        }
    }

    fn run_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                for n in &l.names {
                    self.define(n);
                }
                if let Some(t) = &l.ty {
                    self.run_type(t);
                }
                for v in &l.values {
                    self.run_expr(v);
                }
            }
            Stmt::Assign {
                targets,
                values,
                span,
                ..
            } => {
                // An assignment target is a USE, never a declaration: Go has
                // no declare-on-assign, so `undefined_var = 3` is exactly the
                // `undefined: undefined_var` class this pass exists for.
                for t in targets {
                    self.run_place(t);
                }
                for v in values {
                    self.run_expr(v);
                }
                let _ = span;
            }
            Stmt::Expr { expr, .. } => self.run_expr(expr),
            Stmt::Send { chan, value, .. } => {
                self.run_expr(chan);
                self.run_expr(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.run_stmt(i);
                }
                self.run_expr(cond);
                self.run_block(then);
                if let Some(o) = otherwise {
                    self.run_stmt(o);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    hy_syntax::ForKind::Infinite => {}
                    hy_syntax::ForKind::While { cond } => self.run_expr(cond),
                    hy_syntax::ForKind::Range { var, lo, hi, .. } => {
                        self.define(var);
                        self.run_expr(lo);
                        self.run_expr(hi);
                    }
                    hy_syntax::ForKind::Iterate { vars, expr } => {
                        for v in vars {
                            self.define(v);
                        }
                        self.run_expr(expr);
                    }
                }
                self.run_block(body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.run_expr(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.run_expr(callee);
                for a in args {
                    self.run_expr(&a.value);
                }
            }
            Stmt::Split {
                base,
                mid,
                left,
                right,
                body,
                ..
            } => {
                // W-015: base and mid are USES; the fork names are fresh
                // definitions scoped to the split body.
                self.run_expr(base);
                self.run_expr(mid);
                self.define(left);
                self.define(right);
                self.run_block(body);
            }
            Stmt::Managed { body, .. } => self.run_block(body),
            Stmt::Match {
                subject,
                arms,
                ..
            } => {
                self.run_expr(subject);
                for arm in arms {
                    if let MatchPattern::Path { binding, .. } = &arm.pattern {
                        if let Some(b) = binding {
                            self.define(b);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.run_expr(g);
                    }
                    self.run_block(&arm.body);
                }
            }
            Stmt::Defer { call, .. } => self.run_expr(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => self.run_block(block),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.run_expr(chan);
                            self.run_expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            for n in names {
                                self.define(n);
                            }
                            self.run_expr(chan);
                        }
                        SelectArmKind::Default => {}
                    }
                    self.run_block(&arm.body);
                }
            }
        }
    }

    fn run_place(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(n) => self.ident(n, p.span),
            PlaceKind::Index { base, index } => {
                self.run_place(base);
                self.run_expr(index);
            }
            PlaceKind::Field { base, .. } => self.run_place(base),
            PlaceKind::AddrOf { base } => self.run_place(base),
        }
    }

    fn run_expr(&mut self, e: &Expr) {
        match e {
            Expr::Ident { name, span } => self.ident(name, *span),
            Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.run_expr(x);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.run_expr(lhs);
                self.run_expr(rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. } => self.run_expr(operand),
            Expr::Receive { chan, .. } => self.run_expr(chan),
            Expr::Index { base, index, .. } => {
                self.run_expr(base);
                self.run_expr(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.run_expr(base);
                if let Some(l) = lo {
                    self.run_expr(l);
                }
                if let Some(h) = hi {
                    self.run_expr(h);
                }
            }
            // A DIRECT ident base of a dotted access is exempt: that shape is
            // the package-prefix surface (`fmt.println`, spike-10's `hex.encode`,
            // any std root the emitter's usage-driven import collection maps),
            // and the emitter's contract is "unknown roots still work — they
            // emit verbatim and get an import" (hy-emit is_package_root). The
            // checker cannot tell a real root from a typo'd one, so a dotted
            // first segment stays with the go toolchain; everything bare
            // (callees, plain reads, assignment targets, array lengths) is
            // checked here.
            Expr::Field { base, .. } => {
                let bare = matches!(
                    crate::model::unwrap_parens(base),
                    Expr::Ident { .. }
                );
                if !bare {
                    self.run_expr(base);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.run_expr(callee);
                for a in args {
                    self.run_expr(&a.value);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.run_type(t);
                }
                for f in fields {
                    self.run_expr(&f.value);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.run_type(t);
                }
                for i in items {
                    self.run_expr(i);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.run_type(key);
                self.run_type(value);
                for (k, v) in entries {
                    self.run_expr(k);
                    self.run_expr(v);
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.define(&p.name);
                    self.run_type(&p.ty);
                }
                for r in results {
                    if let Some(n) = &r.name {
                        self.define(n);
                    }
                    self.run_type(&r.ty);
                }
                self.run_block(body);
            }
        }
    }

    /// Type positions are NOT name-checked here (foreign/qualified spellings
    /// resolve through their own tables) — but an expression can hide inside
    /// a type: the `[N]T` array length, which is a real value-position use
    /// (`[undefined_const]int` fails `go build` exactly like any other
    /// undefined name).
    fn run_type(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named { .. } => {}
            TypeExpr::Pointer { to, .. } => self.run_type(to),
            TypeExpr::Slice { elem, .. } => self.run_type(elem),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.run_expr(l);
                }
                self.run_type(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.run_type(key);
                self.run_type(value);
            }
            TypeExpr::Chan { elem, .. } | TypeExpr::Option { elem, .. } => self.run_type(elem),
            TypeExpr::Fn { params, results, .. } => {
                for p in params {
                    self.run_type(&p.ty);
                }
                for r in results {
                    self.run_type(r);
                }
            }
        }
    }
}

/// File-level tables every scope can see: predeclared names, builtin package
/// prefixes, imports, and every top-level declaration (fns, consts, structs,
/// enums + their variants, type aliases) plus type-parameter names on
/// struct/enum declarations. Enum variant names are file-wide on purpose: a
/// bare variant is a value the same way `none` is, and the emitter's
/// enum/match refusal is a separate, already-covered failure.
fn collect_file_tables(file: &hy_syntax::File) -> HashSet<String> {
    let mut s: HashSet<String> = PREDECLARED.iter().map(|n| n.to_string()).collect();
    for p in crate::foreign::BUILTIN_PACKAGES {
        s.insert((*p).to_string());
    }
    for d in &file.decls {
        match d {
            Decl::Import { path, .. } => {
                let root = path.rsplit('/').next().unwrap_or(path.as_str());
                s.insert(root.to_string());
            }
            Decl::Const(c) => {
                s.insert(c.name.clone());
            }
            Decl::Struct(st) => {
                s.insert(st.name.clone());
                for tp in &st.type_params {
                    s.insert(tp.name.clone());
                }
            }
            Decl::Enum(e) => {
                s.insert(e.name.clone());
                for v in &e.variants {
                    s.insert(v.name.clone());
                }
            }
            Decl::TypeAlias(a) => {
                s.insert(a.name.clone());
            }
            Decl::Fn(f) => {
                s.insert(f.name.clone());
            }
        }
    }
    s
}

fn seed_fn_scope(f: &FnDecl, allowed: &mut HashSet<String>) {
    if let Some(r) = &f.receiver {
        allowed.insert(r.name.clone());
    }
    for p in &f.params {
        allowed.insert(p.name.clone());
    }
    for t in &f.type_params {
        allowed.insert(t.name.clone());
    }
    for r in &f.results {
        if let Some(n) = &r.name {
            allowed.insert(n.clone());
        }
    }
}

fn decl_signature_types<'a>(d: &'a Decl) -> Vec<&'a TypeExpr> {
    let mut out: Vec<&TypeExpr> = Vec::new();
    match d {
        Decl::Const(c) => {
            if let Some(t) = &c.ty {
                out.push(t);
            }
        }
        Decl::Struct(st) => {
            for f in &st.fields {
                out.push(&f.ty);
            }
        }
        Decl::Enum(e) => {
            for v in &e.variants {
                if let Some(t) = &v.payload {
                    out.push(t);
                }
            }
        }
        Decl::TypeAlias(a) => out.push(&a.ty),
        Decl::Fn(f) => {
            if let Some(r) = &f.receiver {
                out.push(&r.ty);
            }
            for p in &f.params {
                out.push(&p.ty);
            }
            for r in &f.results {
                out.push(&r.ty);
            }
        }
        Decl::Import { .. } => {}
    }
    out
}

impl Checker<'_> {
    /// File-level half of HYB-E1001: const initializers and every expression
    /// that hides inside a declaration's types (array lengths). Called once
    /// from `check_file` after the tables are built. Function BODIES are not
    /// scanned here — `check_fn_undefined_names` owns those (per-fn scope).
    pub(crate) fn check_file_undefined_names(&mut self) {
        let mut allowed = collect_file_tables(self.file);
        let consts: Vec<(&Expr, Option<&TypeExpr>)> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Const(c) => Some((&c.value, c.ty.as_ref())),
                _ => None,
            })
            .collect();
        let types: Vec<&TypeExpr> = self
            .file
            .decls
            .iter()
            .flat_map(decl_signature_types)
            .collect();
        let path = &self.path;
        let packages = &self.packages;
        let diags = &mut self.diags;
        let mut scan = Scan {
            path,
            packages,
            allowed: &mut allowed,
            diags,
            mode: Mode::Check,
        };
        for (v, t) in consts {
            if let Some(t) = t {
                scan.run_type(t);
            }
            scan.run_expr(v);
        }
        for t in types {
            scan.run_type(t);
        }
    }

    /// Function-body half of HYB-E1001 (issue #228 family A). Two walks over
    /// the same body: COLLECT records every name the function declares
    /// anywhere (including closure params/locals), CHECK reports each
    /// value-position ident and assignment-target root outside that set.
    pub(crate) fn check_fn_undefined_names(&mut self, f: &FnDecl) {
        let Some(body) = &f.body else { return };
        let mut allowed = collect_file_tables(self.file);
        seed_fn_scope(f, &mut allowed);
        {
            let mut scan = Scan {
                path: &self.path,
                packages: &self.packages,
                allowed: &mut allowed,
                diags: &mut self.diags,
                mode: Mode::Collect,
            };
            scan.run_block(body);
        }
        let mut scan = Scan {
            path: &self.path,
            packages: &self.packages,
            allowed: &mut allowed,
            diags: &mut self.diags,
            mode: Mode::Check,
        };
        scan.run_block(body);
    }
}
