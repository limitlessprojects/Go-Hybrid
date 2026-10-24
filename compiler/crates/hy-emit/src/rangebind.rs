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
    Block, Expr, ForKind, InterpPart, MatchPattern, PlaceKind, SelectArmKind, Stmt, TypeExpr,
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

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Checks whether `s` reads `target`, accounting for parser-folded bracket
/// arguments (`fns[i]`, `fns[grid[i]]`, `fns[cfg.slots[i]]`) produced when
/// a bracketed expression is followed by `(` or `{`. Field/method segments
/// immediately preceded by `.` are excluded because they name members rather
/// than variable bindings.
fn ident_reads_target(s: &str, target: &str, is_field_name: bool) -> bool {
    if !is_field_name && s == target {
        return true;
    }
    if !s.contains('[') || target.is_empty() {
        return false;
    }
    let bytes = s.as_bytes();
    let tbytes = target.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if is_ident_byte(bytes[i]) {
            let start = i;
            i += 1;
            while i < bytes.len() && is_ident_byte(bytes[i]) {
                i += 1;
            }
            let is_field =
                (start == 0 && is_field_name) || (start > 0 && bytes[start - 1] == b'.');
            if !is_field && &bytes[start..i] == tbytes {
                return true;
            }
        } else {
            i += 1;
        }
    }
    false
}

pub(crate) fn expr_reads(e: &Expr, target: &str) -> bool {
    match e {
        Expr::Ident { name, .. } => ident_reads_target(name, target, false),
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
        Expr::Field { base, name, .. } => {
            expr_reads(base, target) || ident_reads_target(name, target, true)
        }
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
                        return *inclusive && expr_reads(hi, target);
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
