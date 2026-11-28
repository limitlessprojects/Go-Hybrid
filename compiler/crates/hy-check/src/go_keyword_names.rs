//! go_keyword_names.rs — HYB-E1002: a declaration named `default` or
//! prelude-reserved names (`none`, `option`, `hy_pop`, `hy_n5_make`,
//! and `set` when the prelude type is used) (GUARANTEES Row 13, accepted ⇒ buildable).
//!
//! `default` is the one Go keyword Hybrid does not reserve. LX3 reserves
//! the rest (`switch`, `range`, `goto`, `fallthrough`, `case`, … — #103,
//! #104, #228), and `map` has its contextual rule. `default` stays an
//! identifier on purpose: it is contextual at a select arm head
//! (`default:`, SPEC-LANGUAGE §8.3), and LX3 keeps it out of the reserved
//! list. But hy-emit spells user names verbatim. Every declaration named
//! `default` therefore lands in the emitted Go as the keyword, and gofmt and
//! `go build` refuse the file:
//!
//! ```text
//! default := make(chan int, 1)      // go: expected '}', found 'default'
//! fn f(default int) int { … }       // go: expected ')', found 'default'
//! struct p { default int }          // go: expected '}', found 'default'
//! ```
//!
//! Before this pass `hyb check` accepted every one of those programs with 0
//! diagnostics. A USE of `default` with no declaration is already HYB-E1001,
//! so rejecting each declaration site closes the class. The sites are
//! locals, params, named results, receivers, type params, fn, method,
//! struct, alias, and const names, struct fields, for-in and range
//! variables, select-receive names, match bindings, and closure params.
//!
//! Enum variants are left alone: their emission is a separate surface.
//!
//! The same pass also rejects declarations named `none`, `option`,
//! `hy_pop`, and `hy_n5_make` (and `set` when the set prelude type is used)
//! to prevent collisions with emitted Go prelude helpers and types
//! (issues #777, #4695).

use crate::model::Checker;
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, Decl, Expr, File, FnResult, ForKind, InterpPart, MatchPattern, Param, Place, PlaceKind,
    SelectArmKind, Stmt, TypeExpr, TypeParam,
};

const GO_ONLY_KEYWORD: &str = "default";

fn file_uses_set_prelude(file: &File) -> bool {
    fn type_uses_set(ty: &TypeExpr) -> bool {
        match ty {
            TypeExpr::Named { name, .. } => name.starts_with("set[") || name == "set",
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. }
            | TypeExpr::Option { elem: to, .. } => type_uses_set(to),
            TypeExpr::Array { elem, .. } => type_uses_set(elem),
            TypeExpr::Map { key, value, .. } => type_uses_set(key) || type_uses_set(value),
            TypeExpr::Fn { params, results, .. } => {
                params.iter().any(|p| type_uses_set(&p.ty)) || results.iter().any(type_uses_set)
            }
        }
    }

    fn expr_uses_set(e: &Expr) -> bool {
        match e {
            Expr::StructLit { ty: Some(ty), .. } => type_uses_set(ty),
            Expr::Paren { inner, .. } => expr_uses_set(inner),
            Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
                InterpPart::Expr(x) => expr_uses_set(x),
                _ => false,
            }),
            Expr::Binary { lhs, rhs, .. } => expr_uses_set(lhs) || expr_uses_set(rhs),
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => expr_uses_set(operand),
            Expr::Receive { chan, .. } => expr_uses_set(chan),
            Expr::Index { base, index, .. } => expr_uses_set(base) || expr_uses_set(index),
            Expr::Slice { base, lo, hi, .. } => {
                expr_uses_set(base)
                    || lo.as_ref().is_some_and(|l| expr_uses_set(l))
                    || hi.as_ref().is_some_and(|h| expr_uses_set(h))
            }
            Expr::Field { base, .. } => expr_uses_set(base),
            Expr::Call { callee, args, .. } => {
                expr_uses_set(callee) || args.iter().any(|a| expr_uses_set(&a.value))
            }
            Expr::SliceLit { elem, items, .. } => {
                elem.as_ref().is_some_and(|e| type_uses_set(e)) || items.iter().any(expr_uses_set)
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                type_uses_set(key)
                    || type_uses_set(value)
                    || entries
                        .iter()
                        .any(|(k, v)| expr_uses_set(k) || expr_uses_set(v))
            }
            Expr::FnLit { body, .. } => block_uses_set(body),
            _ => false,
        }
    }

    fn block_uses_set(b: &Block) -> bool {
        b.stmts.iter().any(stmt_uses_set)
    }

    fn stmt_uses_set(s: &Stmt) -> bool {
        match s {
            Stmt::Local(l) => {
                l.ty.as_ref().is_some_and(type_uses_set) || l.values.iter().any(expr_uses_set)
            }
            Stmt::Assign { values, .. } => values.iter().any(expr_uses_set),
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => expr_uses_set(expr),
            Stmt::Send { chan, value, .. } => expr_uses_set(chan) || expr_uses_set(value),
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                init.as_ref().is_some_and(|i| stmt_uses_set(i))
                    || expr_uses_set(cond)
                    || block_uses_set(then)
                    || otherwise.as_ref().is_some_and(|o| stmt_uses_set(o))
            }
            Stmt::For { kind, body, .. } => {
                let kind_matches = match kind {
                    ForKind::While { cond } => expr_uses_set(cond),
                    ForKind::Range { lo, hi, .. } => expr_uses_set(lo) || expr_uses_set(hi),
                    ForKind::Iterate { expr, .. } => expr_uses_set(expr),
                    ForKind::Infinite => false,
                };
                kind_matches || block_uses_set(body)
            }
            Stmt::Task { callee, args, .. } => {
                expr_uses_set(callee) || args.iter().any(|a| expr_uses_set(&a.value))
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => block_uses_set(body),
            Stmt::Match { subject, arms, .. } => {
                expr_uses_set(subject) || arms.iter().any(|a| block_uses_set(&a.body))
            }
            Stmt::Select { arms, .. } => arms.iter().any(|a| block_uses_set(&a.body)),
            Stmt::Return { values, .. } => values.iter().any(expr_uses_set),
            Stmt::Break { .. } | Stmt::Continue { .. } => false,
        }
    }

    for d in &file.decls {
        match d {
            Decl::Const(c) => {
                if c.ty.as_ref().is_some_and(type_uses_set) || expr_uses_set(&c.value) {
                    return true;
                }
            }
            Decl::Struct(s) => {
                if s.fields.iter().any(|f| type_uses_set(&f.ty)) {
                    return true;
                }
            }
            Decl::TypeAlias(t) => {
                if type_uses_set(&t.ty) {
                    return true;
                }
            }
            Decl::Fn(f) => {
                if f.params.iter().any(|p| type_uses_set(&p.ty))
                    || f.results.iter().any(|r| type_uses_set(&r.ty))
                    || f.receiver.as_ref().is_some_and(|r| type_uses_set(&r.ty))
                    || f.body.as_ref().is_some_and(block_uses_set)
                {
                    return true;
                }
            }
            Decl::Import { .. } | Decl::Enum(_) => {}
        }
    }
    false
}

impl<'a> Checker<'a> {
    pub(crate) fn check_go_keyword_names(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => {
                    self.kw_name(&c.name, "const", c.span);
                    if let Some(ty) = &c.ty {
                        self.kw_type(ty);
                    }
                    self.kw_expr(&c.value);
                }
                Decl::Struct(s) => {
                    self.kw_name(&s.name, "struct", s.span);
                    self.kw_type_params(&s.type_params, s.span);
                    for f in &s.fields {
                        self.kw_name(&f.name, "struct field", f.span);
                        self.kw_type(&f.ty);
                    }
                }
                Decl::TypeAlias(t) => {
                    self.kw_name(&t.name, "type alias", t.span);
                    self.kw_type(&t.ty);
                }
                Decl::Fn(f) => {
                    let what = if f.receiver.is_some() { "method" } else { "fn" };
                    self.kw_name(&f.name, what, f.span);
                    if let Some(r) = &f.receiver {
                        self.kw_name(&r.name, "receiver", f.span);
                        self.kw_type(&r.ty);
                    }
                    self.kw_type_params(&f.type_params, f.span);
                    self.kw_params(&f.params);
                    self.kw_results(&f.results, f.span);
                    if let Some(b) = &f.body {
                        self.kw_block(b);
                    }
                }
                Decl::Import { .. } | Decl::Enum(_) => {}
            }
        }
    }

    fn kw_name(&mut self, name: &str, what: &str, span: Span) {
        if name == "none" {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E1002",
                    self.path.clone(),
                    span,
                    format!(
                        "{what} named `none` — `none` is Hybrid's option constructor, so hy-emit cannot treat the name as an ordinary binding and `hyb build` fails (HYB-E1002)"
                    ),
                )
                .with_notes(vec![
                    "why: hy-emit spells every bare Ident `none` as the option constructor (`return none` against option[T], or `not yet supported` elsewhere), so a declaration named `none` is accepted by check and then fails emit — GUARANTEES Row 13".to_string(),
                    "fix: rename it (`none_val`, `absent`, …); keep `none` for `return none` / match arms on `option[T]`".to_string(),
                ]),
            );
            return;
        }
        if name == "option" {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E1002",
                    self.path.clone(),
                    span,
                    format!(
                        "{what} named `option` — `option` is Hybrid's reserved prelude option type (HYB-E1002)"
                    ),
                )
                .with_notes(vec![
                    "why: hy-emit emits `type option[v any] struct` into the Go package block, so declaring `option` collides with the prelude — GUARANTEES Row 13".to_string(),
                    "fix: rename the declaration to an unreserved identifier".to_string(),
                ]),
            );
            return;
        }
        if name == "hy_pop" {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E1002",
                    self.path.clone(),
                    span,
                    format!(
                        "{what} named `hy_pop` — `hy_pop` is a reserved runtime prelude helper, so declaring it collides with slice pop lowering and `hyb build` fails (HYB-E1002)"
                    ),
                )
                .with_notes(vec![
                    "why: hy-emit lowers slice `.pop()` to `hy_pop(&s)` and emits `func hy_pop[t any](s *[]t) t`, so a declaration named `hy_pop` collides in Go package or local scope — GUARANTEES Row 13".to_string(),
                    "fix: rename the declaration to an unreserved identifier".to_string(),
                ]),
            );
            return;
        }
        if name == "hy_n5_make" {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E1002",
                    self.path.clone(),
                    span,
                    format!(
                        "{what} named `hy_n5_make` — `hy_n5_make` is a reserved runtime prelude helper, so declaring it collides with slice allocation lowering and `hyb build` fails (HYB-E1002)"
                    ),
                )
                .with_notes(vec![
                    "why: hy-emit emits `func hy_n5_make[t any](...)` for slice allocation, so a declaration named `hy_n5_make` collides with the prelude in Go package block — GUARANTEES Row 13".to_string(),
                    "fix: rename the declaration to an unreserved identifier".to_string(),
                ]),
            );
            return;
        }
        if name == "set" && matches!(what, "fn" | "const" | "type alias" | "type") && file_uses_set_prelude(self.file) {
            self.diags.push(
                Diagnostic::error(
                    "HYB-E1002",
                    self.path.clone(),
                    span,
                    format!(
                        "{what} named `set` — `set` is Hybrid's built-in set type used in this package, so declaring it collides with `type set` in the emitted Go (HYB-E1002)"
                    ),
                )
                .with_notes(vec![
                    "why: hy-emit emits `type set[t comparable] struct` into the Go package block when sets are used, so a declaration named `set` collides in Go — GUARANTEES Row 13".to_string(),
                    "fix: rename the declaration to an unreserved identifier".to_string(),
                ]),
            );
            return;
        }
        if name != GO_ONLY_KEYWORD {
            return;
        }
        self.diags.push(
            Diagnostic::error(
                "HYB-E1002",
                self.path.clone(),
                span,
                format!(
                    "{what} named `default` — `default` is a Go keyword, so the emitted Go cannot \
                     declare it and `hyb build` fails (HYB-E1002)"
                ),
            )
            .with_notes(vec![
                "why: Hybrid keeps `default` unreserved because it is contextual at a select arm head (`default:`), but hy-emit spells names verbatim, so a declaration named `default` reaches Go as the keyword (gofmt: `expected '}', found 'default'`) — GUARANTEES Row 13".to_string(),
                "fix: rename it (`fallback`, `dflt`, `default_ch`, …); `default` is only meaningful as a select arm head".to_string(),
            ]),
        );
    }

    fn kw_type_params(&mut self, tps: &[TypeParam], span: Span) {
        for tp in tps {
            self.kw_name(&tp.name, "type parameter", span);
        }
    }

    fn kw_params(&mut self, params: &[Param]) {
        for p in params {
            self.kw_name(&p.name, "parameter", p.span);
            self.kw_type(&p.ty);
        }
    }

    fn kw_results(&mut self, results: &[FnResult], span: Span) {
        for r in results {
            if let Some(n) = &r.name {
                self.kw_name(n, "named result", span);
            }
            self.kw_type(&r.ty);
        }
    }

    fn kw_type(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Named { .. } => {}
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. }
            | TypeExpr::Option { elem: to, .. } => self.kw_type(to),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(e) = len {
                    self.kw_expr(e);
                }
                self.kw_type(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.kw_type(key);
                self.kw_type(value);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                self.kw_params(params);
                for r in results {
                    self.kw_type(r);
                }
            }
        }
    }

    fn kw_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.kw_stmt(s);
        }
    }

    fn kw_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                for n in &l.names {
                    self.kw_name(n, "local", l.span);
                }
                if let Some(ty) = &l.ty {
                    self.kw_type(ty);
                }
                for v in &l.values {
                    self.kw_expr(v);
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for t in targets {
                    self.kw_place(t);
                }
                for v in values {
                    self.kw_expr(v);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => self.kw_expr(expr),
            Stmt::Send { chan, value, .. } => {
                self.kw_expr(chan);
                self.kw_expr(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.kw_stmt(i);
                }
                self.kw_expr(cond);
                self.kw_block(then);
                if let Some(o) = otherwise {
                    self.kw_stmt(o);
                }
            }
            Stmt::For { kind, body, span, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.kw_expr(cond),
                    ForKind::Range { var, lo, hi, .. } => {
                        self.kw_name(var, "loop variable", *span);
                        self.kw_expr(lo);
                        self.kw_expr(hi);
                    }
                    ForKind::Iterate { vars, expr } => {
                        for v in vars {
                            self.kw_name(v, "loop variable", *span);
                        }
                        self.kw_expr(expr);
                    }
                }
                self.kw_block(body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.kw_expr(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.kw_expr(callee);
                for a in args {
                    self.kw_expr(&a.value);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => self.kw_block(body),
            Stmt::Match { subject, arms, .. } => {
                self.kw_expr(subject);
                for a in arms {
                    if let MatchPattern::Path {
                        binding: Some(b), ..
                    } = &a.pattern
                    {
                        self.kw_name(b, "match binding", a.body.span);
                    }
                    if let Some(g) = &a.guard {
                        self.kw_expr(g);
                    }
                    self.kw_block(&a.body);
                }
            }
            Stmt::Select { arms, span, .. } => {
                for a in arms {
                    match &a.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.kw_expr(chan);
                            self.kw_expr(value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            for n in names {
                                self.kw_name(n, "select receive binding", *span);
                            }
                            self.kw_expr(chan);
                        }
                        SelectArmKind::Default => {}
                    }
                    self.kw_block(&a.body);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn kw_place(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.kw_place(base);
                self.kw_expr(index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => self.kw_place(base),
        }
    }

    /// Expressions only matter for the closures they contain (a closure's
    /// params and body are declaration sites of their own).
    fn kw_expr(&mut self, e: &Expr) {
        match e {
            Expr::FnLit {
                params,
                results,
                body,
                span,
            } => {
                self.kw_params(params);
                self.kw_results(results, *span);
                self.kw_block(body);
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Paren { inner, .. } => self.kw_expr(inner),
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.kw_expr(x);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.kw_expr(lhs);
                self.kw_expr(rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.kw_expr(operand),
            Expr::Receive { chan, .. } => self.kw_expr(chan),
            Expr::Index { base, index, .. } => {
                self.kw_expr(base);
                self.kw_expr(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.kw_expr(base);
                if let Some(l) = lo {
                    self.kw_expr(l);
                }
                if let Some(h) = hi {
                    self.kw_expr(h);
                }
            }
            Expr::Field { base, .. } => self.kw_expr(base),
            Expr::Call { callee, args, .. } => {
                self.kw_expr(callee);
                for a in args {
                    self.kw_expr(&a.value);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(ty) = ty {
                    self.kw_type(ty);
                }
                for f in fields {
                    self.kw_expr(&f.value);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(elem) = elem {
                    self.kw_type(elem);
                }
                for i in items {
                    self.kw_expr(i);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.kw_type(key);
                self.kw_type(value);
                for (k, v) in entries {
                    self.kw_expr(k);
                    self.kw_expr(v);
                }
            }
        }
    }
}
