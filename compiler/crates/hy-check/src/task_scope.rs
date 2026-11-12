//! task_scope.rs — Structured Task Scopes (`scope { ... }`) & Affine `JoinHandle` (#1669).
use std::collections::HashMap;
use hy_diag::{Diagnostic, Span};
use hy_syntax::{Block, Expr, PlaceKind, SelectArmKind, Stmt};
use crate::model::{unwrap_parens, Checker};
use crate::walk::FlatInfo;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Status { Unjoined, Partial, Consumed }
#[derive(Clone)]
struct Info { status: Status, span: Span, scope_id: usize }

pub(crate) fn check_fn_task_scopes(ck: &mut Checker<'_>, body: &Block, flat: &FlatInfo<'_>) {
    let idx_of: HashMap<usize, usize> = flat.stmts.iter().map(|(s, i)| (*s as *const Stmt as usize, *i)).collect();
    let _ = verify_block(ck, body, flat, &idx_of, &mut HashMap::new());
}

fn verify_block(ck: &mut Checker<'_>, b: &Block, flat: &FlatInfo<'_>, idx_of: &HashMap<usize, usize>, h: &mut HashMap<String, Info>) -> bool {
    b.stmts.iter().fold(false, |div, s| verify_stmt(ck, s, flat, idx_of, h) || div)
}

fn verify_stmt(ck: &mut Checker<'_>, stmt: &Stmt, flat: &FlatInfo<'_>, idx_of: &HashMap<usize, usize>, handles: &mut HashMap<String, Info>) -> bool {
    let idx = idx_of.get(&(stmt as *const Stmt as usize)).copied().unwrap_or(0);
    let in_mgd = flat.in_managed(idx);
    match stmt {
        Stmt::Block { block, span } => {
            let is_scope = flat.scope_end.contains_key(&idx);
            let div = verify_block(ck, block, flat, idx_of, handles);
            if is_scope {
                if !div && !in_mgd {
                    let mut leaked: Vec<_> = handles.iter().filter(|(_, i)| i.scope_id == idx && i.status != Status::Consumed).map(|(k, i)| (k.clone(), i.span)).collect();
                    leaked.sort_by_key(|(_, sp)| (sp.start.line, sp.start.col));
                    for (n, _) in leaked {
                        emit(ck, "HYB-E3501", *span, format!("unjoined task escape: JoinHandle `{n}` is unjoined at `scope` exit (HYB-E3501)"));
                    }
                }
                handles.retain(|_, i| i.scope_id != idx);
            }
            div
        }
        Stmt::Task { span, .. } => {
            if flat.in_scope(idx) && !in_mgd {
                emit(ck, "HYB-E3501", *span, "unjoined task escape: bare `task` inside `scope` discards JoinHandle (HYB-E3501)".into());
            }
            false
        }
        Stmt::Local(l) => {
            for (i, v) in l.values.iter().enumerate() {
                check_cb(ck, v, in_mgd);
                if is_spawn(v) {
                    let name = l.names.get(i).map(|s| s.as_str()).unwrap_or("_");
                    if name == "_" {
                        if flat.in_scope(idx) && !in_mgd {
                            emit(ck, "HYB-E3501", l.span, "unjoined task escape: `_ := task ...` discards JoinHandle (HYB-E3501)".into());
                        }
                    } else if let Some((scope_id, _)) = flat.enclosing_scope(idx) {
                        check_overwrite(ck, name, l.span, handles, in_mgd);
                        handles.insert(name.into(), Info { status: Status::Unjoined, span: l.span, scope_id });
                    }
                } else {
                    check_expr(ck, v, l.span, handles, in_mgd);
                }
            }
            false
        }
        Stmt::Assign { targets, values, span, .. } => {
            for t in targets {
                if let PlaceKind::Ident(n) = &t.kind { check_overwrite(ck, n, *span, handles, in_mgd); }
            }
            for v in values {
                check_cb(ck, v, in_mgd);
                if is_spawn(v) && flat.in_scope(idx) && !in_mgd {
                    emit(ck, "HYB-E3501", *span, "unjoined task escape: `task` assigned outside local binding (HYB-E3501)".into());
                } else {
                    check_expr(ck, v, *span, handles, in_mgd);
                }
            }
            false
        }
        Stmt::Expr { expr, span } => {
            check_cb(ck, expr, in_mgd);
            if !consume_call(ck, expr, *span, handles) { check_expr(ck, expr, *span, handles, in_mgd); }
            false
        }
        Stmt::Send { chan, value, span, .. } => {
            check_expr(ck, chan, *span, handles, in_mgd);
            check_expr(ck, value, *span, handles, in_mgd);
            false
        }
        Stmt::Return { values, span } => {
            for v in values {
                if let Expr::Ident { name, span: vs } = unwrap_parens(v) {
                    if handles.get(name).is_some_and(|h| h.status == Status::Consumed) {
                        emit(ck, "HYB-E3502", *vs, format!("affine JoinHandle `{name}` used after consumption (HYB-E3502)"));
                    }
                }
            }
            check_exit_unjoined(ck, *span, handles, in_mgd);
            true
        }
        Stmt::Break { span, .. } | Stmt::Continue { span, .. } => {
            check_exit_unjoined(ck, *span, handles, in_mgd);
            true
        }
        Stmt::If { init, cond, then, otherwise, .. } => {
            if let Some(i) = init { verify_stmt(ck, i, flat, idx_of, handles); }
            check_expr(ck, cond, cond.span(), handles, in_mgd);
            let (mut th, mut eh) = (handles.clone(), handles.clone());
            let td = verify_block(ck, then, flat, idx_of, &mut th);
            let ed = otherwise.as_ref().is_some_and(|o| verify_stmt(ck, o, flat, idx_of, &mut eh));
            merge_branches(handles, &[(th, td), (eh, ed)]);
            td && ed
        }
        Stmt::For { body, .. } => { let _ = verify_block(ck, body, flat, idx_of, handles); false }
        Stmt::Managed { body, .. } => verify_block(ck, body, flat, idx_of, handles),
        Stmt::Match { subject, arms, .. } => {
            check_expr(ck, subject, subject.span(), handles, in_mgd);
            let br: Vec<_> = arms.iter().map(|a| { let mut h = handles.clone(); let d = verify_block(ck, &a.body, flat, idx_of, &mut h); (h, d) }).collect();
            merge_and_div(handles, &br)
        }
        Stmt::Select { arms, .. } => {
            let br: Vec<_> = arms.iter().map(|a| {
                if let SelectArmKind::Send { chan, value, .. } = &a.kind {
                    let sp = chan.span().to(value.span());
                    check_expr(ck, chan, sp, handles, in_mgd);
                    check_expr(ck, value, sp, handles, in_mgd);
                }
                let mut h = handles.clone();
                let d = verify_block(ck, &a.body, flat, idx_of, &mut h);
                (h, d)
            }).collect();
            merge_and_div(handles, &br)
        }
        Stmt::Defer { call, span } => {
            check_cb(ck, call, in_mgd);
            let _ = consume_call(ck, call, *span, handles);
            false
        }
    }
}

fn check_overwrite(ck: &mut Checker<'_>, name: &str, span: Span, handles: &HashMap<String, Info>, in_mgd: bool) {
    if !in_mgd && handles.get(name).is_some_and(|h| h.status != Status::Consumed) {
        emit(ck, "HYB-E3501", span, format!("unjoined task escape: JoinHandle `{name}` overwritten before join (HYB-E3501)"));
    }
}

fn check_exit_unjoined(ck: &mut Checker<'_>, span: Span, handles: &HashMap<String, Info>, in_mgd: bool) {
    if !in_mgd {
        if let Some((n, _)) = handles.iter().find(|(_, i)| i.status != Status::Consumed) {
            let n = n.clone();
            emit(ck, "HYB-E3501", span, format!("unjoined task escape: control exits `scope` while JoinHandle `{n}` is unjoined (HYB-E3501)"));
        }
    }
}

fn consume_call(ck: &mut Checker<'_>, e: &Expr, span: Span, handles: &mut HashMap<String, Info>) -> bool {
    let Some((h, act)) = extract_consume(e) else { return false };
    if let Some(info) = handles.get_mut(h) {
        if info.status != Status::Unjoined {
            emit(ck, "HYB-E3502", span, format!("affine JoinHandle `{h}` consumed more than once (`{act}`) (HYB-E3502)"));
        } else {
            info.status = Status::Consumed;
        }
    }
    true
}

fn merge_and_div(base: &mut HashMap<String, Info>, br: &[(HashMap<String, Info>, bool)]) -> bool {
    if br.is_empty() { return false; }
    merge_branches(base, br);
    br.iter().all(|(_, d)| *d)
}
fn merge_branches(base: &mut HashMap<String, Info>, branches: &[(HashMap<String, Info>, bool)]) {
    for (name, info) in base.iter_mut() {
        let (mut any_c, mut all_c, mut live) = (false, true, false);
        for (bh, _) in branches.iter().filter(|(_, d)| !*d) {
            live = true;
            match bh.get(name).map(|x| x.status) {
                Some(Status::Consumed) => any_c = true,
                Some(Status::Partial) => { any_c = true; all_c = false; }
                _ => all_c = false,
            }
        }
        if live && any_c { info.status = if all_c { Status::Consumed } else { Status::Partial }; }
    }
}
fn is_spawn(e: &Expr) -> bool {
    matches!(unwrap_parens(e), Expr::Call { callee, .. } if matches!(unwrap_parens(callee), Expr::Ident { name, .. } if name == "__hy_task_spawn"))
}
fn extract_consume(e: &Expr) -> Option<(&str, &str)> {
    let Expr::Call { callee, args, .. } = unwrap_parens(e) else { return None };
    match unwrap_parens(callee) {
        Expr::Ident { name, .. } if matches!(name.as_str(), "join" | "cancel" | "drain") => {
            if let Expr::Ident { name: h, .. } = unwrap_parens(&args.first()?.value) { Some((h, name)) } else { None }
        }
        Expr::Field { base, name, .. } if matches!(name.as_str(), "join" | "cancel" | "drain") => {
            let tgt = if args.is_empty() { unwrap_parens(base) } else { unwrap_parens(&args.first()?.value) };
            if let Expr::Ident { name: h, .. } = tgt { Some((h, name)) } else { None }
        }
        _ => None,
    }
}
fn check_expr(ck: &mut Checker<'_>, e: &Expr, sp: Span, handles: &HashMap<String, Info>, in_mgd: bool) {
    match unwrap_parens(e) {
        Expr::Ident { name, span } => {
            if let Some(i) = handles.get(name) {
                if i.status == Status::Consumed {
                    emit(ck, "HYB-E3502", *span, format!("affine JoinHandle `{name}` used after consumption (HYB-E3502)"));
                } else if !in_mgd {
                    emit(ck, "HYB-E3501", sp, format!("unjoined task escape: JoinHandle `{name}` escapes lexical `scope` (HYB-E3501)"));
                }
            }
        }
        Expr::Move { operand, .. } | Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } | Expr::Try { operand, .. } => check_expr(ck, operand, sp, handles, in_mgd),
        Expr::Binary { lhs, rhs, .. } => { check_expr(ck, lhs, sp, handles, in_mgd); check_expr(ck, rhs, sp, handles, in_mgd); }
        Expr::Call { args, .. } => { for a in args { check_expr(ck, &a.value, sp, handles, in_mgd); } }
        _ => {}
    }
}
fn check_cb(ck: &mut Checker<'_>, e: &Expr, in_mgd: bool) {
    if in_mgd { return; }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        let foreign = matches!(unwrap_parens(callee), Expr::Field { .. });
        for a in args {
            if foreign {
                if let Expr::FnLit { body, .. } = unwrap_parens(&a.value) { check_cb_block(ck, body); continue; }
            }
            check_cb(ck, &a.value, in_mgd);
        }
    }
}
fn check_cb_block(ck: &mut Checker<'_>, block: &Block) {
    for s in &block.stmts {
        match s {
            Stmt::Task { span, .. } => emit(ck, "HYB-E3503", *span, "task spawned in foreign callback without `scope` or `managed` (HYB-E3503)".into()),
            Stmt::Local(l) if l.values.iter().any(is_spawn) => emit(ck, "HYB-E3503", l.span, "task spawned in foreign callback without `scope` or `managed` (HYB-E3503)".into()),
            Stmt::Block { block: b, span } if span.start != b.span.start => {
                let wrap = Block { stmts: vec![s.clone()], span: *span };
                let cb_flat = crate::walk::flatten(&wrap);
                check_fn_task_scopes(ck, &wrap, &cb_flat);
            }
            Stmt::Block { block: b, .. } | Stmt::For { body: b, .. } => check_cb_block(ck, b),
            Stmt::If { then, otherwise, .. } => {
                check_cb_block(ck, then);
                if let Some(Stmt::Block { block: eb, .. }) = otherwise.as_deref() { check_cb_block(ck, eb); }
            }
            _ => {}
        }
    }
}
fn emit(ck: &mut Checker<'_>, code: &str, span: Span, msg: String) {
    ck.diags.push(Diagnostic::error(code, ck.path.clone(), span, msg));
}
