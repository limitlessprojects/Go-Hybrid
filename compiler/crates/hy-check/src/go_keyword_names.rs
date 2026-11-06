//! go_keyword_names.rs — HYB-E1002: a declaration named `default`
//! (GUARANTEES Row 13, accepted ⇒ buildable).
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

use crate::model::Checker;
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, Decl, Expr, FnResult, ForKind, InterpPart, MatchPattern, Param, Place, PlaceKind,
    SelectArmKind, Stmt, TypeExpr, TypeParam,
};

const GO_ONLY_KEYWORD: &str = "default";

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
