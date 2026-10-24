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
    BinOp, Block, Expr, ForKind, InterpPart, MatchPattern, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

/// Does `body` reference `name` anywhere that is not shadowed by an inner
/// binding of `name`?
pub fn body_reads(body: &Block, name: &str) -> bool {
    if name == "_" {
        return false;
    }
    block_reads(body, name)
}

fn block_reads(b: &Block, target: &str) -> bool {
    for st in &b.stmts {
        if let Stmt::Local(l) = st {
            if l.ty.as_ref().is_some_and(|t| ty_reads(t, target))
                || l.values.iter().any(|v| expr_reads(v, target))
            {
                return true;
            }
            if l.names.iter().any(|n| n == target) {
                return false;
            }
            continue;
        }
        if stmt_reads(st, target) {
            return true;
        }
    }
    false
}

fn ty_reads(t: &TypeExpr, target: &str) -> bool {
    match t {
        TypeExpr::Named { .. } => false,
        TypeExpr::Pointer { to, .. } => ty_reads(to, target),
        TypeExpr::Slice { elem, .. }
        | TypeExpr::Chan { elem, .. }
        | TypeExpr::Option { elem, .. } => ty_reads(elem, target),
        TypeExpr::Array { len, elem, .. } => {
            len.as_ref().is_some_and(|l| expr_reads(l, target)) || ty_reads(elem, target)
        }
        TypeExpr::Map { key, value, .. } => ty_reads(key, target) || ty_reads(value, target),
        TypeExpr::Fn {
            params, results, ..
        } => {
            params.iter().any(|p| ty_reads(&p.ty, target))
                || results.iter().any(|r| ty_reads(r, target))
        }
    }
}

pub(crate) fn expr_reads(e: &Expr, target: &str) -> bool {
    match e {
        Expr::Ident { name, .. } => name == target,
        Expr::Lit { .. } => false,
        Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
            InterpPart::Expr(x) => expr_reads(x, target),
            InterpPart::Lit(_) => false,
        }),
        Expr::Binary { lhs, rhs, .. } => expr_reads(lhs, target) || expr_reads(rhs, target),
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. } => expr_reads(operand, target),
        Expr::Receive { chan, .. } => expr_reads(chan, target),
        Expr::Index { base, index, .. } => expr_reads(base, target) || expr_reads(index, target),
        Expr::Slice { base, lo, hi, .. } => {
            expr_reads(base, target)
                || lo.as_ref().is_some_and(|x| expr_reads(x, target))
                || hi.as_ref().is_some_and(|x| expr_reads(x, target))
        }
        Expr::Field { base, .. } => expr_reads(base, target),
        Expr::Call { callee, args, .. } => {
            expr_reads(callee, target) || args.iter().any(|a| expr_reads(&a.value, target))
        }
        Expr::StructLit { ty, fields, .. } => {
            ty.as_ref().is_some_and(|t| ty_reads(t, target))
                || fields.iter().any(|f| expr_reads(&f.value, target))
        }
        Expr::SliceLit { elem, items, .. } => {
            elem.as_ref().is_some_and(|t| ty_reads(t, target))
                || items.iter().any(|v| expr_reads(v, target))
        }
        Expr::MapLit {
            key,
            value,
            entries,
            ..
        } => {
            ty_reads(key, target)
                || ty_reads(value, target)
                || entries
                    .iter()
                    .any(|(k, v)| expr_reads(k, target) || expr_reads(v, target))
        }
        Expr::FnLit {
            params,
            results,
            body,
            ..
        } => {
            if params.iter().any(|p| ty_reads(&p.ty, target))
                || results.iter().any(|r| ty_reads(&r.ty, target))
            {
                return true;
            }
            if params.iter().any(|p| p.name == target)
                || results.iter().any(|r| r.name.as_deref() == Some(target))
            {
                return false;
            }
            block_reads(body, target)
        }
        Expr::Paren { inner, .. } => expr_reads(inner, target),
    }
}

fn place_reads(p: &hy_syntax::Place, target: &str) -> bool {
    match &p.kind {
        PlaceKind::Ident(n) => n == target,
        PlaceKind::Index { base, index } => {
            place_reads(base, target) || expr_reads(index, target)
        }
        PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => place_reads(base, target),
    }
}

fn stmt_reads(s: &Stmt, target: &str) -> bool {
    match s {
        Stmt::Local(l) => {
            l.ty.as_ref().is_some_and(|t| ty_reads(t, target))
                || l.values.iter().any(|v| expr_reads(v, target))
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            targets.iter().any(|t| place_reads(t, target))
                || values.iter().any(|v| expr_reads(v, target))
        }
        Stmt::Expr { expr, .. } => expr_reads(expr, target),
        Stmt::Send { chan, value, .. } => expr_reads(chan, target) || expr_reads(value, target),
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                if stmt_reads(i, target) {
                    return true;
                }
                if let Stmt::Local(l) = i.as_ref() {
                    if l.names.iter().any(|n| n == target) {
                        return false;
                    }
                }
            }
            expr_reads(cond, target)
                || block_reads(then, target)
                || otherwise.as_deref().is_some_and(|o| stmt_reads(o, target))
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Iterate { vars, expr } => {
                    if expr_reads(expr, target) {
                        return true;
                    }
                    if vars.iter().any(|v| v == target) {
                        return false;
                    }
                }
                ForKind::While { cond } => {
                    if expr_reads(cond, target) {
                        return true;
                    }
                }
                ForKind::Range {
                    var,
                    lo,
                    hi,
                    inclusive,
                } => {
                    if expr_reads(lo, target) {
                        return true;
                    }
                    if var == target {
                        return (*inclusive && expr_reads(hi, target))
                            || (!*inclusive && expr_reads_before_range_var(hi, target));
                    }
                    if expr_reads(hi, target) {
                        return true;
                    }
                }
                ForKind::Infinite => {}
            }
            block_reads(body, target)
        }
        Stmt::Return { values, .. } => values.iter().any(|v| expr_reads(v, target)),
        Stmt::Task { callee, args, .. } => {
            expr_reads(callee, target) || args.iter().any(|a| expr_reads(&a.value, target))
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => block_reads(body, target),
        Stmt::Match { subject, arms, .. } => {
            if expr_reads(subject, target) {
                return true;
            }
            let is_option_match = !arms.is_empty()
                && !matches!(arms.first().map(|a| &a.pattern), Some(MatchPattern::Wildcard))
                && arms.iter().all(|a| {
                    matches!(
                        &a.pattern,
                        MatchPattern::Path {
                            segments,
                            binding: Some(_),
                        } if segments == &["some"]
                    ) || matches!(
                        &a.pattern,
                        MatchPattern::Path { segments, .. } if segments == &["none"]
                    ) || matches!(&a.pattern, MatchPattern::Wildcard)
                });
            if is_option_match {
                let mut some_arms: Vec<(&str, &hy_syntax::MatchArm)> = Vec::new();
                let mut none_arm: Option<&hy_syntax::MatchArm> = None;
                let mut wildcard_arm: Option<&hy_syntax::MatchArm> = None;
                for arm in arms {
                    match &arm.pattern {
                        MatchPattern::Path {
                            segments,
                            binding: Some(b),
                        } if segments == &["some"] => {
                            some_arms.push((b.as_str(), arm));
                        }
                        MatchPattern::Path { segments, .. } if segments == &["none"] => {
                            none_arm = Some(arm);
                        }
                        MatchPattern::Wildcard => {
                            wildcard_arm = Some(arm);
                        }
                        _ => {}
                    }
                }
                if some_arms.is_empty() {
                    return false;
                }
                let reachable_len = some_arms
                    .iter()
                    .position(|(_, arm)| arm.guard.is_none())
                    .map(|idx| idx + 1)
                    .unwrap_or(some_arms.len());
                let reachable_some_arms = &some_arms[..reachable_len];
                let has_unguarded_some = reachable_some_arms
                    .iter()
                    .any(|(_, arm)| arm.guard.is_none());
                for (binding, arm) in reachable_some_arms {
                    if *binding == target {
                        continue;
                    }
                    if arm.guard.as_ref().is_some_and(|g| expr_reads(g, target))
                        || block_reads(&arm.body, target)
                    {
                        return true;
                    }
                }
                if has_unguarded_some {
                    let else_arm = none_arm.or(wildcard_arm);
                    return else_arm.is_some_and(|arm| block_reads(&arm.body, target));
                }
                return none_arm.is_some_and(|arm| block_reads(&arm.body, target))
                    || wildcard_arm.is_some_and(|arm| block_reads(&arm.body, target));
            }
            for arm in arms {
                if matches!(
                    &arm.pattern,
                    MatchPattern::Path {
                        segments,
                        binding: Some(b),
                    } if segments == &["some"] && b == target
                ) {
                    continue;
                }
                if arm.guard.as_ref().is_some_and(|g| expr_reads(g, target))
                    || block_reads(&arm.body, target)
                {
                    return true;
                }
            }
            false
        }
        Stmt::Defer { call, .. } => expr_reads(call, target),
        Stmt::Break { .. } | Stmt::Continue { .. } => false,
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Recv { names, chan } => {
                        if expr_reads(chan, target) {
                            return true;
                        }
                        if names.iter().any(|n| n == target) {
                            continue;
                        }
                    }
                    SelectArmKind::Send { chan, value, .. } => {
                        if expr_reads(chan, target) || expr_reads(value, target) {
                            return true;
                        }
                    }
                    SelectArmKind::Default => {}
                }
                if block_reads(&arm.body, target) {
                    return true;
                }
            }
            false
        }
    }
}

/// Mirrors `Emitter::hoist_tries_expr` for an exclusive range bound `hi` when
/// the range variable shadows `target`: `hoist_tries_expr(hi)` runs BEFORE
/// `for <var> := <lo>; <var> < <hi>; <var>++`, so any subexpression hoisted out
/// of `hi` resolves `target` against the outer binding rather than `<var>`.
fn expr_reads_before_range_var(e: &Expr, target: &str) -> bool {
    match e {
        Expr::Try { operand, .. } => expr_reads(operand, target),
        Expr::Binary { op, lhs, rhs, .. } => {
            if matches!(op, BinOp::And | BinOp::Or) && crate::Emitter::expr_has_hoistable_try(rhs) {
                expr_reads(e, target)
            } else if crate::Emitter::expr_has_hoistable_try(rhs) {
                expr_reads_in_hoisted_order(lhs, target)
                    || expr_reads_before_range_var(rhs, target)
            } else {
                expr_reads_before_range_var(lhs, target)
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => expr_reads_before_range_var(operand, target),
        Expr::Interp { parts, .. } => parts.iter().enumerate().any(|(idx, p)| match p {
            InterpPart::Expr(x) => {
                let later_has_try = parts[idx + 1..].iter().any(|rest| {
                    matches!(rest, InterpPart::Expr(rx) if crate::Emitter::expr_has_hoistable_try(rx))
                });
                if later_has_try {
                    expr_reads_in_hoisted_order(x, target)
                } else {
                    expr_reads_before_range_var(x, target)
                }
            }
            InterpPart::Lit(_) => false,
        }),
        Expr::Call { callee, args, .. } => {
            let any_arg_has_try = args
                .iter()
                .any(|a| crate::Emitter::expr_has_hoistable_try(&a.value));
            let callee_reads = if any_arg_has_try {
                match callee.as_ref() {
                    Expr::Field { base, .. } => expr_reads_in_hoisted_order(base, target),
                    Expr::Call { .. } => expr_reads_in_hoisted_order(callee, target),
                    _ => expr_reads_before_range_var(callee, target),
                }
            } else {
                expr_reads_before_range_var(callee, target)
            };
            callee_reads
                || args.iter().enumerate().any(|(idx, a)| {
                    let later_has_try = args[idx + 1..]
                        .iter()
                        .any(|rest| crate::Emitter::expr_has_hoistable_try(&rest.value));
                    if later_has_try {
                        expr_reads_in_hoisted_order(&a.value, target)
                    } else {
                        expr_reads_before_range_var(&a.value, target)
                    }
                })
        }
        Expr::Index { base, index, .. } => {
            if crate::Emitter::expr_has_hoistable_try(index) {
                expr_reads_in_hoisted_order(base, target)
                    || expr_reads_before_range_var(index, target)
            } else {
                expr_reads_before_range_var(base, target)
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            let lo_try = lo
                .as_deref()
                .is_some_and(crate::Emitter::expr_has_hoistable_try);
            let hi_try = hi
                .as_deref()
                .is_some_and(crate::Emitter::expr_has_hoistable_try);
            let base_reads = if lo_try || hi_try {
                expr_reads_in_hoisted_order(base, target)
            } else {
                expr_reads_before_range_var(base, target)
            };
            let lo_reads = lo.as_deref().is_some_and(|l| {
                if hi_try {
                    expr_reads_in_hoisted_order(l, target)
                } else {
                    expr_reads_before_range_var(l, target)
                }
            });
            let hi_reads = hi
                .as_deref()
                .is_some_and(|h| expr_reads_before_range_var(h, target));
            base_reads || lo_reads || hi_reads
        }
        Expr::StructLit { fields, .. } => fields.iter().enumerate().any(|(idx, f)| {
            let later_has_try = fields[idx + 1..]
                .iter()
                .any(|rest| crate::Emitter::expr_has_hoistable_try(&rest.value));
            if later_has_try {
                expr_reads_in_hoisted_order(&f.value, target)
            } else {
                expr_reads_before_range_var(&f.value, target)
            }
        }),
        Expr::SliceLit { items, .. } => items.iter().enumerate().any(|(idx, item)| {
            let later_has_try = items[idx + 1..]
                .iter()
                .any(crate::Emitter::expr_has_hoistable_try);
            if later_has_try {
                expr_reads_in_hoisted_order(item, target)
            } else {
                expr_reads_before_range_var(item, target)
            }
        }),
        Expr::MapLit { entries, .. } => entries.iter().enumerate().any(|(idx, (k, v))| {
            let later_entry_try = entries[idx + 1..].iter().any(|(rk, rv)| {
                crate::Emitter::expr_has_hoistable_try(rk)
                    || crate::Emitter::expr_has_hoistable_try(rv)
            });
            let key_reads = if crate::Emitter::expr_has_hoistable_try(v) || later_entry_try {
                expr_reads_in_hoisted_order(k, target)
            } else {
                expr_reads_before_range_var(k, target)
            };
            let val_reads = if later_entry_try {
                expr_reads_in_hoisted_order(v, target)
            } else {
                expr_reads_before_range_var(v, target)
            };
            key_reads || val_reads
        }),
        Expr::FnLit { .. } | Expr::Lit { .. } | Expr::Ident { .. } => false,
    }
}

fn expr_reads_in_hoisted_order(e: &Expr, target: &str) -> bool {
    if expr_reads_before_range_var(e, target) {
        return true;
    }
    let inner = crate::Emitter::peel_parens(e);
    expr_has_call_or_recv(inner) && expr_reads(e, target)
}

fn expr_has_call_or_recv(e: &Expr) -> bool {
    match e {
        Expr::Call { .. } | Expr::Receive { .. } | Expr::Try { .. } => true,
        Expr::Binary { lhs, rhs, .. } => expr_has_call_or_recv(lhs) || expr_has_call_or_recv(rhs),
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. } => expr_has_call_or_recv(operand),
        Expr::Interp { parts, .. } => parts.iter().any(|p| match p {
            InterpPart::Expr(x) => expr_has_call_or_recv(x),
            InterpPart::Lit(_) => false,
        }),
        Expr::Index { base, index, .. } => {
            expr_has_call_or_recv(base) || expr_has_call_or_recv(index)
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_has_call_or_recv(base)
                || lo.as_deref().is_some_and(expr_has_call_or_recv)
                || hi.as_deref().is_some_and(expr_has_call_or_recv)
        }
        Expr::StructLit { fields, .. } => fields.iter().any(|f| expr_has_call_or_recv(&f.value)),
        Expr::SliceLit { items, .. } => items.iter().any(expr_has_call_or_recv),
        Expr::MapLit { entries, .. } => entries
            .iter()
            .any(|(k, v)| expr_has_call_or_recv(k) || expr_has_call_or_recv(v)),
        Expr::FnLit { .. } | Expr::Lit { .. } | Expr::Ident { .. } => false,
    }
}
