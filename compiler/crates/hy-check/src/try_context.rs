//! E4 — where `?` may appear (SPEC-LANGUAGE §6.1, HYB-E2011).
//!
//! "In any function whose results end in `error` (and in `main`, where it
//! triggers the exit contract E6) … `?` inside a closure binds to the
//! closure's own results, not the enclosing function's (lexical, like
//! `return`)." E11 adds the one other landing spot: a closure literal
//! converted by a target with a propagation contract (`http.handler`).
//!
//! Before this pass nothing enforced E4: `x := f()?` in a fn returning `int`
//! passed `hyb check` and the emitter refused it ("`?` outside main"); a
//! result-less closure inside `main` was ACCEPTED and BUILT, its `?` writing
//! main's hidden exit error and returning from the closure only — `main`
//! kept running after the error (E3b: never a silent swallow).

use crate::model::{unwrap_parens, Checker};
use hy_diag::Diagnostic;
use hy_syntax::{
    Block, Expr, File, FnResult, ForKind, InterpPart, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

/// The conversion targets that carry an E11 propagation contract. "Scope:
/// exactly one target carries a contract at MVP; adding one is a Decision
/// Log entry" — keep in sync with hy-emit's `CONVERSIONS[..].propagate`.
const E11_PROPAGATING_TARGETS: &[&str] = &["http.handler"];

fn results_end_in_error(results: &[FnResult]) -> bool {
    matches!(
        results.last().map(|r| &r.ty),
        Some(TypeExpr::Named { name, .. }) if name == "error"
    )
}

/// What the enclosing function or closure offers a `?`.
#[derive(Clone, Copy)]
enum Ctx<'a> {
    /// `main`, an error-result fn/closure, or an E11 handler closure.
    Ok,
    /// No landing spot; the &str names the enclosing callable for the message.
    NoErrorResult(&'a str),
}

pub(crate) fn check_try_contexts(ck: &mut Checker<'_>, file: &File) {
    for f in file.fns() {
        let Some(body) = &f.body else { continue };
        let is_main = f.name == "main" && f.receiver.is_none();
        let ctx = if is_main || results_end_in_error(&f.results) {
            Ctx::Ok
        } else {
            Ctx::NoErrorResult(f.name.as_str())
        };
        walk_block(ck, body, ctx);
    }
}

fn walk_block(ck: &mut Checker<'_>, b: &Block, ctx: Ctx<'_>) {
    for s in &b.stmts {
        walk_stmt(ck, s, ctx);
    }
}

fn walk_stmt(ck: &mut Checker<'_>, s: &Stmt, ctx: Ctx<'_>) {
    match s {
        Stmt::Local(l) => l.values.iter().for_each(|v| walk_expr(ck, v, ctx)),
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                walk_place(ck, &t.kind, ctx);
            }
            values.iter().for_each(|v| walk_expr(ck, v, ctx));
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => walk_expr(ck, expr, ctx),
        Stmt::Send { chan, value, .. } => {
            walk_expr(ck, chan, ctx);
            walk_expr(ck, value, ctx);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                walk_stmt(ck, i, ctx);
            }
            walk_expr(ck, cond, ctx);
            walk_block(ck, then, ctx);
            if let Some(o) = otherwise {
                walk_stmt(ck, o, ctx);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::While { cond } => walk_expr(ck, cond, ctx),
                ForKind::Range { lo, hi, .. } => {
                    walk_expr(ck, lo, ctx);
                    walk_expr(ck, hi, ctx);
                }
                ForKind::Iterate { expr, .. } => walk_expr(ck, expr, ctx),
                ForKind::Infinite => {}
            }
            walk_block(ck, body, ctx);
        }
        Stmt::Return { values, .. } => values.iter().for_each(|v| walk_expr(ck, v, ctx)),
        Stmt::Task { callee, args, .. } => {
            walk_expr(ck, callee, ctx);
            args.iter().for_each(|a| walk_expr(ck, &a.value, ctx));
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => walk_block(ck, body, ctx),
        Stmt::Match { subject, arms, .. } => {
            walk_expr(ck, subject, ctx);
            for a in arms {
                if let Some(g) = &a.guard {
                    walk_expr(ck, g, ctx);
                }
                walk_block(ck, &a.body, ctx);
            }
        }
        Stmt::Select { arms, .. } => {
            for a in arms {
                match &a.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        walk_expr(ck, chan, ctx);
                        walk_expr(ck, value, ctx);
                    }
                    SelectArmKind::Recv { chan, .. } => walk_expr(ck, chan, ctx),
                    SelectArmKind::Default => {}
                }
                walk_block(ck, &a.body, ctx);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn walk_place(ck: &mut Checker<'_>, p: &PlaceKind, ctx: Ctx<'_>) {
    match p {
        PlaceKind::Ident(_) => {}
        PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
            walk_place(ck, &base.kind, ctx)
        }
        PlaceKind::Index { base, index } => {
            walk_place(ck, &base.kind, ctx);
            walk_expr(ck, index, ctx);
        }
    }
}

/// A closure literal converted by an E11 propagating target:
/// `http.handler(fn(w, r) { … })`.
fn is_e11_conversion_call(callee: &Expr) -> bool {
    if let Expr::Field { base, name, .. } = unwrap_parens(callee) {
        if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
            let full = format!("{}.{}", pkg, name);
            return E11_PROPAGATING_TARGETS.contains(&full.as_str());
        }
    }
    false
}

fn walk_fnlit(ck: &mut Checker<'_>, e: &Expr, e11: bool) {
    if let Expr::FnLit { results, body, .. } = e {
        // E4: lexical, like `return` — the closure's OWN results decide.
        let ctx = if e11 || results_end_in_error(results) {
            Ctx::Ok
        } else {
            Ctx::NoErrorResult("closure")
        };
        walk_block(ck, body, ctx);
    }
}

fn walk_expr(ck: &mut Checker<'_>, e: &Expr, ctx: Ctx<'_>) {
    match e {
        Expr::Try { operand, span } => {
            if let Ctx::NoErrorResult(owner) = ctx {
                let owner_text = if owner == "closure" {
                    "this closure".to_string()
                } else {
                    format!("`{}`", owner)
                };
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E2011",
                        ck.path.clone(),
                        *span,
                        format!(
                            "`?` needs an `error` result to return through, but {} has none (E4, HYB-E2011)",
                            owner_text
                        ),
                    )
                    .with_notes(vec![
                        "E4: `?` may appear only in a function or closure whose results end in `error`, in `main` (E6 exit contract), or in an `http.handler` closure (E11)".to_string(),
                        "a closure's `?` binds to the closure's own results, never the enclosing function's (lexical, like `return`)".to_string(),
                        "fix: add `error` as the last result and return it, or handle the error in place: `v, err := f()` then `if err != nil { … }`".to_string(),
                    ]),
                );
            }
            walk_expr(ck, operand, ctx);
        }
        Expr::FnLit { .. } => walk_fnlit(ck, e, false),
        Expr::Call { callee, args, .. } => {
            walk_expr(ck, callee, ctx);
            let e11 = is_e11_conversion_call(callee);
            for a in args {
                if e11 && matches!(unwrap_parens(&a.value), Expr::FnLit { .. }) {
                    walk_fnlit(ck, unwrap_parens(&a.value), true);
                } else {
                    walk_expr(ck, &a.value, ctx);
                }
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    walk_expr(ck, x, ctx);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            walk_expr(ck, lhs, ctx);
            walk_expr(ck, rhs, ctx);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Receive { chan: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Paren { inner: operand, .. } => walk_expr(ck, operand, ctx),
        Expr::Index { base, index, .. } => {
            walk_expr(ck, base, ctx);
            walk_expr(ck, index, ctx);
        }
        Expr::Slice { base, lo, hi, .. } => {
            walk_expr(ck, base, ctx);
            if let Some(l) = lo {
                walk_expr(ck, l, ctx);
            }
            if let Some(h) = hi {
                walk_expr(ck, h, ctx);
            }
        }
        Expr::StructLit { fields, .. } => fields.iter().for_each(|f| walk_expr(ck, &f.value, ctx)),
        Expr::SliceLit { items, .. } => items.iter().for_each(|i| walk_expr(ck, i, ctx)),
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                walk_expr(ck, k, ctx);
                walk_expr(ck, v, ctx);
            }
        }
    }
}
