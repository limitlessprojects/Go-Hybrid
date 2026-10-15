//! tryop.rs — the E4 / T20 / E3b rules for the try operator `?`.
//!
//! SPEC-LANGUAGE E4 pins two TYPE errors that were never implemented
//! (issue #155):
//!
//! > `?` on an expression that is not an error-returning call is a type
//! > error; `?` on `option` is a type error (T20) — absence and failure
//! > stay distinct.
//!
//! and E3b pins WHERE a `?` may propagate:
//!
//! > Where the propagation cannot land (no error result, not main, not an
//! > E11 handler), it is a compile error — never a silent swallow.
//!
//! Before this pass `Expr::Try` was only a transparent operand-walk in
//! `shape.rs`, so all three were accepted with 0 diagnostics and the
//! emitted Go then failed to build (`assignment mismatch: 2 variables but
//! count returns 1 value`) — breaking the accepted-should-emit-buildable-Go
//! contract, and in the option case silently conflating absence with
//! failure.
//!
//! Deliberately PERMISSIVE, like the rest of Tier 0: a try whose operand
//! does not resolve to a local `fn` declaration carries no opinion (the
//! result of a foreign or method call may well be an `error`), and a
//! context that is not a declared function with explicit results is never
//! called out. What the pass does reject, it rejects with certainty.

use hy_diag::{Span, Pos};
use hy_syntax::{Block, Expr, FnDecl, ForKind, Stmt, TypeExpr};

use crate::model::Checker;

/// Entry point for a declared function.
pub(crate) fn check_try_ops_fn(ck: &mut Checker<'_>, f: &FnDecl) {
    let Some(body) = &f.body else { return };
    let results = classify(&f.results);
    check_block(ck, body, &results, Some(&f.name));
}

/// What the results of the enclosing (or target) function can host.
struct RetInfo {
    /// True when at least one declared result is `error` — this is what a
    /// propagation needs to land on (E3b).
    has_error: bool,
    /// True when the FIRST result is `option[T]` — the T20 confusion.
    first_is_option: bool,
    /// The function declares results explicitly (a bare `fn() { … }`
    /// closure literal does not).
    explicit: bool,
}

fn classify(results: &[hy_syntax::FnResult]) -> RetInfo {
    let explicit = !results.is_empty();
    let has_error = results.iter().any(|r| is_error(&r.ty));
    let first_is_option = matches!(results.first().map(|r| &r.ty), Some(TypeExpr::Option { .. }));
    RetInfo {
        has_error,
        first_is_option,
        explicit,
    }
}

fn is_error(ty: &TypeExpr) -> bool {
    match ty {
        TypeExpr::Named { name, .. } => name == "error",
        _ => false,
    }
}

fn check_block(ck: &mut Checker<'_>, block: &Block, ret: &RetInfo, fn_name: Option<&str>) {
    for s in &block.stmts {
        check_stmt(ck, s, ret, fn_name);
    }
}

fn check_stmt(ck: &mut Checker<'_>, stmt: &Stmt, ret: &RetInfo, fn_name: Option<&str>) {
    match stmt {
        Stmt::Local(l) => {
            // `code, body := http.get(url)?` — the try sits in a value.
            for v in &l.values {
                check_expr(ck, v, ret, fn_name);
            }
        }
        Stmt::Assign { values, .. } => {
            for v in values {
                check_expr(ck, v, ret, fn_name);
            }
        }
        Stmt::Expr { expr, .. } => check_expr(ck, expr, ret, fn_name),
        Stmt::Send { chan, value, .. } => {
            check_expr(ck, chan, ret, fn_name);
            check_expr(ck, value, ret, fn_name);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                check_stmt(ck, i, ret, fn_name);
            }
            check_expr(ck, cond, ret, fn_name);
            check_block(ck, then, ret, fn_name);
            if let Some(o) = otherwise {
                check_stmt(ck, o, ret, fn_name);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::While { cond } => check_expr(ck, cond, ret, fn_name),
                // `for i in 0..n` and `for k, v in m` carry expressions too.
                ForKind::Range { lo, hi, .. } => {
                    check_expr(ck, lo, ret, fn_name);
                    check_expr(ck, hi, ret, fn_name);
                }
                ForKind::Iterate { expr, .. } => check_expr(ck, expr, ret, fn_name),
                ForKind::Infinite => {}
            }
            check_block(ck, body, ret, fn_name);
        }
        Stmt::Return { values, .. } => {
            for v in values {
                check_expr(ck, v, ret, fn_name);
            }
        }
        Stmt::Task { callee, args, .. } => {
            check_expr(ck, callee, ret, fn_name);
            for a in args {
                check_expr(ck, &a.value, ret, fn_name);
            }
        }
        Stmt::Managed { body, .. } => check_block(ck, body, ret, fn_name),
        Stmt::Match { subject, arms, .. } => {
            check_expr(ck, subject, ret, fn_name);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    check_expr(ck, g, ret, fn_name);
                }
                check_block(ck, &arm.body, ret, fn_name);
            }
        }
        Stmt::Defer { call, .. } => check_expr(ck, call, ret, fn_name),
        Stmt::Block { block, .. } => check_block(ck, block, ret, fn_name),
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    hy_syntax::SelectArmKind::Send { chan, value } => {
                        check_expr(ck, chan, ret, fn_name);
                        check_expr(ck, value, ret, fn_name);
                    }
                    hy_syntax::SelectArmKind::Recv { chan, .. } => {
                        check_expr(ck, chan, ret, fn_name)
                    }
                    hy_syntax::SelectArmKind::Default => {}
                }
                check_block(ck, &arm.body, ret, fn_name);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn check_expr(ck: &mut Checker<'_>, e: &Expr, ret: &RetInfo, fn_name: Option<&str>) {
    match e {
        Expr::Try { operand, span } => {
            check_one_try(ck, operand, (span.start.line, span.start.col), ret, fn_name);
            check_expr(ck, operand, ret, fn_name);
        }
        Expr::FnLit {
            results, body, span, ..
        } => {
            // A closure hoists its OWN tries: the propagation must land on
            // the closure's own results, never the enclosing function's
            // (E4's lexical rule). An untyped closure literal (`fn(w …)` —
            // the A5 perimeter shape) declares nothing, so it carries no
            // opinion.
            let inner = classify(results);
            if inner.explicit || !results.is_empty() {
                check_block(ck, body, &inner, None);
            } else {
                // untyped: still look for the operand/option type errors,
                // but do not judge the propagation context
                check_block(ck, body, &inner, None);
            }
            let _ = span;
        }
        Expr::Binary { lhs, rhs, .. } => {
            check_expr(ck, lhs, ret, fn_name);
            check_expr(ck, rhs, ret, fn_name);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Paren { inner: operand, .. } => check_expr(ck, operand, ret, fn_name),
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let hy_syntax::InterpPart::Expr(x) = p {
                    check_expr(ck, x, ret, fn_name);
                }
            }
        }
        Expr::Call { callee, args, .. } => {
            check_expr(ck, callee, ret, fn_name);
            for a in args {
                check_expr(ck, &a.value, ret, fn_name);
            }
        }
        Expr::Receive { chan, .. } => check_expr(ck, chan, ret, fn_name),
        Expr::Field { base, .. } => check_expr(ck, base, ret, fn_name),
        Expr::Index { base, index, .. } => {
            check_expr(ck, base, ret, fn_name);
            check_expr(ck, index, ret, fn_name);
        }
        Expr::Slice { base, lo, hi, .. } => {
            check_expr(ck, base, ret, fn_name);
            if let Some(l) = lo {
                check_expr(ck, l, ret, fn_name);
            }
            if let Some(h) = hi {
                check_expr(ck, h, ret, fn_name);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                check_expr(ck, &f.value, ret, fn_name);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                check_expr(ck, i, ret, fn_name);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                check_expr(ck, k, ret, fn_name);
                check_expr(ck, v, ret, fn_name);
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
    }
}

/// The three certain rules.
fn check_one_try(
    ck: &mut Checker<'_>,
    operand: &Expr,
    at: (u32, u32),
    ret: &RetInfo,
    fn_name: Option<&str>,
) {
    let span = Span::point(Pos::new(at.0, at.1));

    // (1) + (2): the OPERAND must be an error-returning call, and `option`
    // is never a failure (T20).
    if let Some(name) = called_name(operand) {
        if let Some(decl) = ck.fns.get(name.as_str()).copied() {
            let target = classify(&decl.results);
            if target.first_is_option {
                ck.diags.push(
                    hy_diag::Diagnostic::error(
                        "HYB-E4004",
                        ck.path.clone(),
                        span,
                        format!(
                            "`?` on `{name}(...)` is a type error: it returns `option`, not `error` (T20)"
                        ),
                    )
                    .with_notes(vec![
                        "absence and failure stay distinct: an `option` is not an error (SPEC-LANGUAGE E4)"
                            .to_string(),
                        "the emitter hoists a two-name binding from a one-result call, so the emitted Go cannot build either"
                            .to_string(),
                        "use `match` (or a `some(...)`/`none()` test) for an `option` result".to_string(),
                    ]),
                );
                return;
            }
            if !target.has_error {
                ck.diags.push(
                    hy_diag::Diagnostic::error(
                        "HYB-E4003",
                        ck.path.clone(),
                        span,
                        format!(
                            "`?` on `{name}(...)` is a type error: the call does not return `error`"
                        ),
                    )
                    .with_notes(vec![
                        format!(
                            "`{name}` declares {} result(s) and none of them is `error` (SPEC-LANGUAGE E4)",
                            decl.results.len()
                        ),
                        "the emitted Go cannot build: `assignment mismatch: 2 variables but ... returns 1 value`"
                            .to_string(),
                        "drop the `?` — the value is already the result you want".to_string(),
                    ]),
                );
                return;
            }

            // (3) E3b: the operand IS a locally-declared error-returning
            // call — so the failure is real and must be able to land. Where
            // the enclosing function or typed closure cannot host it, that
            // is a compile error, never a silent swallow. `main` and
            // untyped closure literals declare nothing and are exempt; a
            // callee the checker cannot resolve (foreign/method) carries no
            // opinion at all, or this would reject the corpus's own
            // `for u in doc.array(\"users\")?` shape.
            if ret.explicit && !ret.has_error {
                let owner = match fn_name {
                    Some(n) => format!("`{n}`"),
                    None => "this closure".to_string(),
                };
                ck.diags.push(
                    hy_diag::Diagnostic::error(
                        "HYB-E4005",
                        ck.path.clone(),
                        span,
                        format!(
                            "`?` cannot propagate here: {owner} declares results and none of them is `error`"
                        ),
                    )
                    .with_notes(vec![
                        "the propagation must land on an `error` result (SPEC-LANGUAGE E3b) — never a silent swallow"
                            .to_string(),
                        "a `?` binds to the results of the function or closure that lexically contains it (E4)"
                            .to_string(),
                        "add an `error` result, or handle the failure here instead of propagating it"
                            .to_string(),
                    ]),
                );
            }
        }
    }

}

/// The name of a plain call callee (`f(...)`), if this operand is one.
fn called_name(e: &Expr) -> Option<String> {
    let inner = crate::model::unwrap_parens(e);
    match inner {
        Expr::Call { callee, .. } => match crate::model::unwrap_parens(callee) {
            Expr::Ident { name, .. } => Some(name.clone()),
            _ => None,
        },
        _ => None,
    }
}
