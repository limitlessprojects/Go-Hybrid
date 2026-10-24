//! missing_return.rs — a function with results must end in a terminating
//! statement (HYB-E1002, `BUILDABLE`, GUARANTEES Row 13).
//!
//! Go rejects `func f() int { }` with `missing return`: every function that
//! declares results — named or not, top-level, method, or closure — must end
//! in a *terminating statement* (Go spec, "Terminating statements"). Before
//! this check `hyb check` accepted such bodies with zero diagnostics and the
//! emitted module failed `go build`.
//!
//! The rule mirrors the spec, applied to the statement shapes the emitter
//! lowers 1:1 to Go:
//!
//!   * `return`, and a call to the builtin `panic(...)`;
//!   * a block (bare, or a `managed` hatch) whose last statement terminates;
//!   * `if` with an `else` where both branches terminate;
//!   * an infinite `for {}` / `loop {}` that no `break` targets;
//!   * `select` / `match` where every arm terminates and (select only) no
//!     `break` targets the select.
//!
//! The check is fail-open on anything it cannot read: an unknown statement
//! shape counts as terminating, so the rule only rejects bodies that Go
//! provably rejects. Every reject case in `tests/missing_return.rs` was
//! first reproduced as a real `go build` failure on the emitted Go.

use crate::model::Checker;
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Block, Expr, FnResult, ForKind, Stmt};

impl Checker<'_> {
    /// HYB-E1002: `results` are declared but `body` can fall off its end.
    pub(crate) fn check_missing_return(
        &mut self,
        what: &str,
        results: &[FnResult],
        body: &Block,
        span: Span,
    ) {
        if results.is_empty() || block_terminates(body) {
            return;
        }
        let sig = results
            .iter()
            .map(|r| match &r.name {
                Some(n) => format!("{} {}", n, r.ty.render()),
                None => r.ty.render(),
            })
            .collect::<Vec<_>>()
            .join(", ");
        let msg = format!(
            "{} declares result(s) `{}` but its body can reach the end without a `return` — Go rejects the emitted function with `missing return` (HYB-E1002)",
            what, sig
        );
        self.diags.push(
            Diagnostic::error("HYB-E1002", self.path.clone(), span, msg).with_notes(vec![
                "why: Go requires every function with results to end in a terminating statement (`return`, `panic(...)`, an `if`/`else` whose branches both terminate, or a `for {}` with no `break`), named results included".to_string(),
                "fix: add a final `return ...` (or `panic(\"unreachable\")` when the end truly cannot be reached)".to_string(),
            ]),
        );
    }
}

/// Go spec "Terminating statements", item 8: a block terminates when its
/// last non-empty statement terminates.
pub(crate) fn block_terminates(b: &Block) -> bool {
    match b.stmts.last() {
        None => false,
        Some(s) => stmt_terminates(s),
    }
}

fn stmt_terminates(s: &Stmt) -> bool {
    match s {
        Stmt::Return { .. } => true,
        Stmt::Expr { expr, .. } => is_panic_call(expr),
        Stmt::Block { block, .. } => block_terminates(block),
        Stmt::Managed { body, .. } => block_terminates(body),
        Stmt::If {
            then, otherwise, ..
        } => match otherwise {
            None => false,
            Some(o) => block_terminates(then) && stmt_terminates(o),
        },
        Stmt::For { kind, body, .. } => match kind {
            // `for cond {}` / ranges / iteration: never terminating in Go.
            ForKind::While { .. } | ForKind::Range { .. } | ForKind::Iterate { .. } => false,
            ForKind::Infinite => !block_breaks_loop(body),
        },
        Stmt::Select { arms, .. } => arms
            .iter()
            .all(|a| block_terminates(&a.body) && !block_breaks_select(&a.body)),
        // Option matches lower to `if … else …`; every arm must terminate.
        // An empty arm list is fail-open (the emitter refuses it anyway).
        Stmt::Match { arms, .. } => {
            arms.is_empty() || arms.iter().all(|a| block_terminates(&a.body))
        }
        Stmt::Local(_)
        | Stmt::Assign { .. }
        | Stmt::Send { .. }
        | Stmt::Task { .. }
        | Stmt::Defer { .. }
        | Stmt::Break { .. }
        | Stmt::Continue { .. } => false,
    }
}

fn is_panic_call(e: &Expr) -> bool {
    match e {
        Expr::Paren { inner, .. } => is_panic_call(inner),
        Expr::Call { callee, .. } => {
            matches!(&**callee, Expr::Ident { name, .. } if name == "panic")
        }
        _ => false,
    }
}

/// Does any `break` in `b` target the enclosing infinite loop?
///
/// The emitter lowers `break` to Go's unlabeled `break` and `break <label>`
/// to the label of the innermost enclosing loop. So an unlabeled break
/// targets this loop unless an inner `for` or `select` captures it; a
/// labeled break targets this loop unless an inner `for` captures it.
/// `match` lowers to `if`/`else` and captures nothing. Closure bodies are
/// separate functions and are not entered.
fn block_breaks_loop(b: &Block) -> bool {
    b.stmts.iter().any(|s| stmt_breaks(s, true, true))
}

/// Does an unlabeled `break` target this `select`? (A labeled break leaves
/// the select for the enclosing loop, which is the loop rule's business.)
fn block_breaks_select(b: &Block) -> bool {
    b.stmts.iter().any(|s| stmt_breaks(s, true, false))
}

/// `unlabeled` / `labeled`: whether a break of that form found here still
/// targets the construct being analysed.
fn stmt_breaks(s: &Stmt, unlabeled: bool, labeled: bool) -> bool {
    match s {
        Stmt::Break { label: None, .. } => unlabeled,
        Stmt::Break { label: Some(_), .. } => labeled,
        Stmt::Block { block, .. } | Stmt::Managed { body: block, .. } => block
            .stmts
            .iter()
            .any(|s| stmt_breaks(s, unlabeled, labeled)),
        Stmt::If {
            then, otherwise, ..
        } => {
            then.stmts
                .iter()
                .any(|s| stmt_breaks(s, unlabeled, labeled))
                || otherwise
                    .as_deref()
                    .is_some_and(|o| stmt_breaks(o, unlabeled, labeled))
        }
        Stmt::Match { arms, .. } => arms.iter().any(|a| {
            a.body
                .stmts
                .iter()
                .any(|s| stmt_breaks(s, unlabeled, labeled))
        }),
        // An inner select captures unlabeled breaks; labeled ones pass through.
        Stmt::Select { arms, .. } => arms
            .iter()
            .any(|a| a.body.stmts.iter().any(|s| stmt_breaks(s, false, labeled))),
        // An inner loop captures every break form.
        Stmt::For { .. } => false,
        _ => false,
    }
}
