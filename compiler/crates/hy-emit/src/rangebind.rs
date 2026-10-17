//! `for a, b in xs` lowers to Go's `for a, b := range xs`. Go's front-end
//! rejects an unread range binding (`declared and not used`), so when one of
//! the two names is never referenced in the loop body the emitted module can
//! never build even though `hyb check` accepted the program (issue #139).
//!
//! The emitter already lowers `for _ in xs` to `for range xs`, so the blank
//! spelling is a familiar shape here. This module decides, for one loop body,
//! which binding names the body actually reads — used only to pick `_` for the
//! ones it does not.
//!
//! Deliberately conservative: a name mentioned ANYWHERE in the body (including
//! inside a closure, a nested loop, a match binding or a select binding) counts
//! as read, so this pass can only ever ADD an `_` where nothing could possibly
//! have read the name. A wrongly-placed `_` would be a semantic bug; a missing
//! one merely leaves the old emission, which Go rejects as it does today.

use hy_syntax::{
    Block, Expr, ForKind, InterpPart, MatchPattern, PlaceKind, SelectArmKind, Stmt,
};
use std::collections::HashSet;

/// Does the loop body mention `name` anywhere?
pub fn body_reads(body: &Block, name: &str) -> bool {
    let mut kata = HashSet::new();
    block_kata(body, &mut kata);
    kata.contains(name)
}

fn block_kata(b: &Block, out: &mut HashSet<String>) {
    for st in &b.stmts {
        stmt_kata(st, out);
    }
}

fn expr_kata(e: &Expr, out: &mut HashSet<String>) {
    match e {
        Expr::Ident { name, .. } => {
            out.insert(name.clone());
        }
        Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(x) = p {
                    expr_kata(x, out);
                }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            expr_kata(lhs, out);
            expr_kata(rhs, out);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. } => expr_kata(operand, out),
        Expr::Receive { chan, .. } => expr_kata(chan, out),
        Expr::Index { base, index, .. } => {
            expr_kata(base, out);
            expr_kata(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_kata(base, out);
            if let Some(x) = lo {
                expr_kata(x, out);
            }
            if let Some(x) = hi {
                expr_kata(x, out);
            }
        }
        Expr::Field { base, .. } => expr_kata(base, out),
        Expr::Call { callee, args, .. } => {
            expr_kata(callee, out);
            for a in args {
                expr_kata(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_kata(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for v in items {
                expr_kata(v, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_kata(k, out);
                expr_kata(v, out);
            }
        }
        Expr::FnLit { body, .. } => block_kata(body, out),
        Expr::Paren { inner, .. } => expr_kata(inner, out),
    }
}

fn place_kata(p: &hy_syntax::Place, out: &mut HashSet<String>) {
    match &p.kind {
        PlaceKind::Ident(n) => {
            out.insert(n.clone());
        }
        PlaceKind::Index { base, index } => {
            place_kata(base, out);
            expr_kata(index, out);
        }
        PlaceKind::Field { base, .. } => place_kata(base, out),
        PlaceKind::AddrOf { base } => place_kata(base, out),
    }
}

fn stmt_kata(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Local(l) => {
            for v in &l.values {
                expr_kata(v, out);
            }
        }
        Stmt::Assign { targets, values, .. } => {
            for t in targets {
                place_kata(t, out);
            }
            for v in values {
                expr_kata(v, out);
            }
        }
        Stmt::Expr { expr, .. } => expr_kata(expr, out),
        Stmt::Send { chan, value, .. } => {
            expr_kata(chan, out);
            expr_kata(value, out);
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                stmt_kata(i, out);
            }
            expr_kata(cond, out);
            block_kata(then, out);
            if let Some(o) = otherwise {
                stmt_kata(o, out);
            }
        }
        Stmt::For { kind, body, .. } => {
            // A nested loop's own bindings are mentions like any other; they
            // only ever suppress an `_`, never introduce one.
            match kind {
                ForKind::Iterate { vars, expr } => {
                    for v in vars {
                        out.insert(v.clone());
                    }
                    expr_kata(expr, out);
                }
                ForKind::While { cond } => expr_kata(cond, out),
                ForKind::Range { var, lo, hi, .. } => {
                    out.insert(var.clone());
                    expr_kata(lo, out);
                    expr_kata(hi, out);
                }
                ForKind::Infinite => {}
            }
            block_kata(body, out);
        }
        Stmt::Return { values, .. } => {
            for v in values {
                expr_kata(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_kata(callee, out);
            for a in args {
                expr_kata(&a.value, out);
            }
        }
        Stmt::Managed { body, .. } => block_kata(body, out),
        Stmt::Split { body, .. } => block_kata(body, out),
        Stmt::Match {
            subject, arms, ..
        } => {
            expr_kata(subject, out);
            for arm in arms {
                if let MatchPattern::Path {
                    binding: Some(b), ..
                } = &arm.pattern
                {
                    out.insert(b.clone());
                }
                if let Some(g) = &arm.guard {
                    expr_kata(g, out);
                }
                block_kata(&arm.body, out);
            }
        }
        Stmt::Defer { call, .. } => expr_kata(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Block { block, .. } => block_kata(block, out),
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Recv { names, chan } => {
                        for n in names {
                            out.insert(n.clone());
                        }
                        expr_kata(chan, out);
                    }
                    SelectArmKind::Send { chan, value } => {
                        expr_kata(chan, out);
                        expr_kata(value, out);
                    }
                    SelectArmKind::Default => {}
                }
                block_kata(&arm.body, out);
            }
        }
    }
}
