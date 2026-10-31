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
//! Package selectors (e.g. `fmt.println`, `strings.split`, imported packages)
//! resolve via the M4/M5 curated allowlist, builtin packages, and explicit
//! imports. A field base that is not a package root must resolve against
//! declared locals, parameters, or file-level declarations; undeclared field
//! bases (e.g. `q.a`) reject with HYB-E1001 (issue #2271 Case 1).
//!
//! Scoping evaluates short variable declaration (`:=`) right-hand sides
//! before introducing new bindings, rejecting self-referential bindings
//! like `ch := ch` with HYB-E1001 (issue #2271 Case 2). Lexical blocks
//! maintain proper scoping for block/loop/arm statements. Type positions
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

struct Scan<'a> {
    path: &'a str,
    packages: &'a HashSet<String>,
    file_decls: &'a HashSet<String>,
    allowed: &'a mut HashSet<String>,
    local_decls: &'a mut HashSet<String>,
    uncurated_imports: &'a HashSet<String>,
    diags: &'a mut Vec<hy_diag::Diagnostic>,
}

impl<'a> Scan<'a> {
    fn define(&mut self, name: &str) {
        self.allowed.insert(name.to_string());
        self.local_decls.insert(name.to_string());
    }

    fn ident(&mut self, name: &str, span: Span) {
        if self.allowed.contains(name) || self.packages.contains(name) {
            return;
        }
        if crate::foreign::BUILTIN_PACKAGES.contains(&name) {
            return;
        }
        // `parser.rs` (`fold_type_args`) folds bracketed type arguments (and
        // single-ident index expressions before `(`) into `Expr::Ident`
        // (`id[int](42)` -> `Ident("id[int]")`, `m[k](42)` -> `Ident("m[k]")`).
        if let Some((head, rest)) = name.split_once('[') {
            let head_ok = self.allowed.contains(head)
                || self.packages.contains(head)
                || crate::foreign::BUILTIN_PACKAGES.contains(&head);
            if head_ok {
                if !self.file_decls.contains(head) {
                    if let Some(inside) = rest.strip_suffix(']') {
                        let trimmed = inside.trim();
                        if !trimmed.is_empty()
                            && trimmed
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                            && !trimmed.chars().next().unwrap().is_ascii_digit()
                        {
                            self.ident(trimmed, span);
                        }
                    }
                }
                return;
            }
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
        let saved_allowed = self.allowed.clone();
        let saved_locals = self.local_decls.clone();
        for s in &b.stmts {
            self.run_stmt(s);
        }
        *self.allowed = saved_allowed;
        *self.local_decls = saved_locals;
    }

    fn run_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                if let Some(t) = &l.ty {
                    self.run_type(t);
                }
                for v in &l.values {
                    self.run_expr(v);
                }
                for n in &l.names {
                    self.define(n);
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
                let saved_allowed = self.allowed.clone();
                let saved_locals = self.local_decls.clone();
                if let Some(i) = init {
                    self.run_stmt(i);
                }
                self.run_expr(cond);
                self.run_block(then);
                if let Some(o) = otherwise {
                    self.run_stmt(o);
                }
                *self.allowed = saved_allowed;
                *self.local_decls = saved_locals;
            }
            Stmt::For { kind, body, .. } => {
                let saved_allowed = self.allowed.clone();
                let saved_locals = self.local_decls.clone();
                match kind {
                    hy_syntax::ForKind::Infinite => {}
                    hy_syntax::ForKind::While { cond } => self.run_expr(cond),
                    hy_syntax::ForKind::Range { var, lo, hi, .. } => {
                        self.run_expr(lo);
                        self.run_expr(hi);
                        self.define(var);
                    }
                    hy_syntax::ForKind::Iterate { vars, expr } => {
                        self.run_expr(expr);
                        for v in vars {
                            self.define(v);
                        }
                    }
                }
                self.run_block(body);
                *self.allowed = saved_allowed;
                *self.local_decls = saved_locals;
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
            Stmt::Managed { body, .. } => self.run_block(body),
            Stmt::Match {
                subject,
                arms,
                ..
            } => {
                self.run_expr(subject);
                for arm in arms {
                    let saved_allowed = self.allowed.clone();
                    let saved_locals = self.local_decls.clone();
                    if let MatchPattern::Path { segments, binding } = &arm.pattern {
                        if segments.len() == 1 {
                            let arm_span = arm
                                .guard
                                .as_ref()
                                .map(|g| g.span())
                                .unwrap_or(arm.body.span);
                            self.ident(&segments[0], arm_span);
                        }
                        if let Some(b) = binding {
                            self.define(b);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        self.run_expr(g);
                    }
                    self.run_block(&arm.body);
                    *self.allowed = saved_allowed;
                    *self.local_decls = saved_locals;
                }
            }
            Stmt::Defer { call, .. } => self.run_expr(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => self.run_block(block),
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    let saved_allowed = self.allowed.clone();
                    let saved_locals = self.local_decls.clone();
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.run_expr(chan);
                            self.run_expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.run_expr(chan);
                            for n in names {
                                self.define(n);
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    self.run_block(&arm.body);
                    *self.allowed = saved_allowed;
                    *self.local_decls = saved_locals;
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
            PlaceKind::Field { base, name } => {
                if let PlaceKind::Ident(base_name) = &base.kind {
                    if !self.local_decls.contains(base_name) {
                        if self.uncurated_imports.contains(base_name)
                            || crate::foreign::is_uncurated_go_stdlib_root(base_name)
                        {
                            let msg = format!(
                                "package `{}` is not in the M5 curated allowlist ({}) (HYB-E1001)",
                                base_name,
                                crate::foreign::CURATED_ALLOWLIST_DISPLAY
                            );
                            self.diags.push(
                                hy_diag::Diagnostic::error(
                                    "HYB-E1001",
                                    self.path.to_string(),
                                    p.span,
                                    msg,
                                ),
                            );
                            return;
                        } else if crate::foreign::is_curated_package_root(base_name) {
                            if !crate::foreign::is_curated_package_member(base_name, name) {
                                let msg = format!(
                                    "`{}.{}` is not a resolved member of curated package `{}` (HYB-E1001)",
                                    base_name, name, base_name
                                );
                                self.diags.push(
                                    hy_diag::Diagnostic::error(
                                        "HYB-E1001",
                                        self.path.to_string(),
                                        p.span,
                                        msg,
                                    ),
                                );
                            }
                            return;
                        }
                    }
                }
                self.run_place(base);
            }
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
            Expr::Field { base, name, span } => {
                let unwrapped = crate::model::unwrap_parens(base);
                if let Expr::Ident { name: root, .. } = unwrapped {
                    if !self.local_decls.contains(root) {
                        if self.uncurated_imports.contains(root)
                            || crate::foreign::is_uncurated_go_stdlib_root(root)
                        {
                            self.diags.push(
                                hy_diag::Diagnostic::error(
                                    "HYB-E1001",
                                    self.path.to_string(),
                                    *span,
                                    format!(
                                        "`{}.{}` cannot be resolved — package `{}` is outside the M5 curated allowlist (allowlist: {}) (HYB-E1001)",
                                        root,
                                        name,
                                        root,
                                        crate::foreign::CURATED_ALLOWLIST_DISPLAY
                                    ),
                                )
                                .with_notes(vec![
                                    "SPEC M5: only packages on the curated allowlist have normalized bindings".to_string(),
                                ]),
                            );
                            return;
                        } else if crate::foreign::is_curated_package_root(root) {
                            if !crate::foreign::is_curated_package_member(root, name) {
                                self.diags.push(
                                    hy_diag::Diagnostic::error(
                                        "HYB-E1001",
                                        self.path.to_string(),
                                        *span,
                                        format!(
                                            "`{}.{}` is not a resolved member of curated package `{}` (HYB-E1001)",
                                            root, name, root
                                        ),
                                    )
                                    .with_notes(vec![
                                        format!(
                                            "check the M4 snake_case spelling for `{}` exports (e.g. `io.eof`, `http.status_ok`)",
                                            root
                                        ),
                                    ]),
                                );
                            }
                            return;
                        } else if self.packages.contains(root)
                            || crate::foreign::BUILTIN_PACKAGES.contains(&root.as_str())
                        {
                            return;
                        }
                    }
                }
                self.run_expr(base);
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
                let mut fn_allowed = self.allowed.clone();
                let mut fn_locals = self.local_decls.clone();
                for p in params {
                    fn_allowed.insert(p.name.clone());
                    fn_locals.insert(p.name.clone());
                }
                for r in results {
                    if let Some(n) = &r.name {
                        fn_allowed.insert(n.clone());
                        fn_locals.insert(n.clone());
                    }
                }
                let mut check = Scan {
                    path: self.path,
                    packages: self.packages,
                    file_decls: self.file_decls,
                    allowed: &mut fn_allowed,
                    local_decls: &mut fn_locals,
                    uncurated_imports: self.uncurated_imports,
                    diags: self.diags,
                };
                for p in params {
                    check.run_type(&p.ty);
                }
                for r in results {
                    check.run_type(&r.ty);
                }
                check.run_block(body);
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

/// Top-level declaration names (functions, structs, enums, variants, type
/// aliases, plus predeclared conversions/builtins) whose `[…]` suffix in a
/// call is generic type instantiation rather than map/slice indexing.
fn collect_file_decl_names(file: &hy_syntax::File) -> HashSet<String> {
    let mut s: HashSet<String> = PREDECLARED.iter().map(|n| n.to_string()).collect();
    for d in &file.decls {
        match d {
            Decl::Struct(st) => {
                s.insert(st.name.clone());
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
            Decl::Import { .. } | Decl::Const(_) => {}
        }
    }
    s
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

fn collect_file_local_decls(file: &hy_syntax::File) -> HashSet<String> {
    let mut s = HashSet::new();
    for d in &file.decls {
        match d {
            Decl::Import { .. } => {}
            Decl::Const(c) => {
                s.insert(c.name.clone());
            }
            Decl::Struct(st) => {
                s.insert(st.name.clone());
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

fn collect_uncurated_imports(file: &hy_syntax::File) -> HashSet<String> {
    let mut s = HashSet::new();
    for d in &file.decls {
        if let Decl::Import { path, .. } = d {
            if !crate::foreign::is_curated_import(path) {
                let root = path.rsplit('/').next().unwrap_or(path.as_str());
                s.insert(root.to_string());
            }
        }
    }
    s
}

fn seed_receiver_type_params(ty: &TypeExpr, allowed: &mut HashSet<String>) {
    match ty {
        TypeExpr::Named { name, .. } => {
            if let Some((_, rest)) = name.split_once('[') {
                if let Some((inside, _)) = rest.rsplit_once(']') {
                    for part in inside.split(',') {
                        if let Some(tp) = part.split_whitespace().next() {
                            if !tp.is_empty() {
                                allowed.insert(tp.to_string());
                            }
                        }
                    }
                }
            }
        }
        TypeExpr::Pointer { to, .. } => seed_receiver_type_params(to, allowed),
        _ => {}
    }
}

fn seed_fn_scope(f: &FnDecl, allowed: &mut HashSet<String>) {
    if let Some(r) = &f.receiver {
        allowed.insert(r.name.clone());
        seed_receiver_type_params(&r.ty, allowed);
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

fn decl_signature_types(d: &Decl) -> Vec<&TypeExpr> {
    let mut out: Vec<&TypeExpr> = Vec::new();
    match d {
        Decl::Const(c) => {
            if let Some(t) = &c.ty {
                out.push(t);
            }
        }
        Decl::Struct(_) | Decl::Fn(_) => {}
        Decl::Enum(e) => {
            for v in &e.variants {
                if let Some(t) = &v.payload {
                    out.push(t);
                }
            }
        }
        Decl::TypeAlias(a) => out.push(&a.ty),
        Decl::Import { .. } => {}
    }
    out
}

impl Checker<'_> {
    /// File-level half of HYB-E1001: M5 curated import allowlist, const
    /// initializers, and every expression that hides inside a declaration's
    /// types (array lengths). Called once from `check_file` after the tables
    /// are built. Function BODIES are not scanned here —
    /// `check_fn_undefined_names` owns those (per-fn scope).
    pub(crate) fn check_file_undefined_names(&mut self) {
        for d in &self.file.decls {
            if let Decl::Import { path, span } = d {
                if !crate::foreign::is_curated_import(path) {
                    self.diags.push(
                        hy_diag::Diagnostic::error(
                            "HYB-E1001",
                            self.path.to_string(),
                            *span,
                            format!(
                                "import {:?} is outside the M5 curated package allowlist \
                                 (allowlist: {}) (HYB-E1001)",
                                path,
                                crate::foreign::CURATED_ALLOWLIST_DISPLAY
                            ),
                        )
                        .with_notes(vec![
                            "SPEC M5: MVP Hybrid imports a curated allowlist of Go packages and pinned Rust crates; anything outside the allowlist is a compile error naming the allowlist, not a silent link failure"
                                .to_string(),
                        ]),
                    );
                }
            }
        }
        let mut allowed = collect_file_tables(self.file);
        let file_decls = collect_file_decl_names(self.file);
        let mut local_decls = collect_file_local_decls(self.file);
        let uncurated_imports = collect_uncurated_imports(self.file);
        let consts: Vec<(&Expr, Option<&TypeExpr>)> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Const(c) => Some((&c.value, c.ty.as_ref())),
                _ => None,
            })
            .collect();
        let structs: Vec<&hy_syntax::StructDecl> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Struct(st) => Some(st),
                _ => None,
            })
            .collect();
        let fns: Vec<&FnDecl> = self
            .file
            .decls
            .iter()
            .filter_map(|d| match d {
                Decl::Fn(f) => Some(f),
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
        {
            let mut scan = Scan {
                path,
                packages,
                file_decls: &file_decls,
                allowed: &mut allowed,
                local_decls: &mut local_decls,
                uncurated_imports: &uncurated_imports,
                diags,
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
        for st_decl in structs {
            let mut struct_allowed = allowed.clone();
            let mut struct_locals = local_decls.clone();
            for tp in &st_decl.type_params {
                struct_allowed.insert(tp.name.clone());
                struct_locals.insert(tp.name.clone());
            }
            let mut scan = Scan {
                path,
                packages,
                file_decls: &file_decls,
                allowed: &mut struct_allowed,
                local_decls: &mut struct_locals,
                uncurated_imports: &uncurated_imports,
                diags,
                };
            for f in &st_decl.fields {
                scan.run_type(&f.ty);
            }
        }
        for f_decl in fns {
            let mut fn_allowed = allowed.clone();
            let mut fn_locals = local_decls.clone();
            seed_fn_scope(f_decl, &mut fn_allowed);
            seed_fn_scope(f_decl, &mut fn_locals);
            let mut scan = Scan {
                path,
                packages,
                file_decls: &file_decls,
                allowed: &mut fn_allowed,
                local_decls: &mut fn_locals,
                uncurated_imports: &uncurated_imports,
                diags,
                };
            if let Some(r) = &f_decl.receiver {
                scan.run_type(&r.ty);
            }
            for p in &f_decl.params {
                scan.run_type(&p.ty);
            }
            for r in &f_decl.results {
                scan.run_type(&r.ty);
            }
        }
    }

    /// Function-body half of HYB-E1001 (issue #228 family A + #658 M4/M5).
    /// Two walks over the same body: COLLECT records every name the function
    /// declares anywhere (including closure params/locals), CHECK reports each
    /// value-position ident, assignment-target root, and unresolved curated
    /// package member.
    pub(crate) fn check_fn_undefined_names(&mut self, f: &FnDecl) {
        let Some(body) = &f.body else { return };
        let mut allowed = collect_file_tables(self.file);
        let file_decls = collect_file_decl_names(self.file);
        let mut local_decls = collect_file_local_decls(self.file);
        let uncurated_imports = collect_uncurated_imports(self.file);
        seed_fn_scope(f, &mut allowed);
        seed_fn_scope(f, &mut local_decls);
        let mut scan = Scan {
            path: &self.path,
            packages: &self.packages,
            file_decls: &file_decls,
            allowed: &mut allowed,
            local_decls: &mut local_decls,
            uncurated_imports: &uncurated_imports,
            diags: &mut self.diags,
            };
        scan.run_block(body);
    }
}
