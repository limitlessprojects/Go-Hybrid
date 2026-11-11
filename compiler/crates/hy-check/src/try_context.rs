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
//!
//! The operand half (E3 "calls only" / E4 "`?` on a non-error call" and
//! "`?` on option") is also HYB-E2011: `1?`, `x.f?`, `foo()?` when `foo`
//! returns a non-error result, and `opt()?` when `opt` returns `option[T]`
//! all passed check and then failed `go build` with an assignment mismatch.

use std::collections::HashMap;

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, Decl, Expr, File, FnDecl, FnResult, ForKind, InterpPart, LocalKind, MatchPattern, Param,
    PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

/// The conversion targets that carry an E11 propagation contract. "Scope:
/// exactly one target carries a contract at MVP; adding one is a Decision
/// Log entry" — keep in sync with hy-emit's `CONVERSIONS[..].propagate`.
const E11_PROPAGATING_TARGETS: &[&str] = &["http.handler"];

fn is_builtin_error_type(ck: &Checker<'_>, ty: &TypeExpr, type_params: &[String]) -> bool {
    let mut cur = ty.clone();
    for _ in 0..8 {
        let TypeExpr::Named { name, .. } = &cur else {
            return false;
        };
        if type_params.iter().any(|tp| tp == name) {
            return false;
        }
        if let Some(next) = ck.type_aliases.get(name.as_str()) {
            cur = next.clone();
        } else {
            break;
        }
    }
    if !matches!(&cur, TypeExpr::Named { name, .. } if name == "error")
        || type_params.iter().any(|tp| tp == "error")
        || ck.structs.contains_key("error")
        || ck.struct_type_params.contains_key("error")
        || ck.enums.contains_key("error")
    {
        return false;
    }
    ck.type_aliases.get("error").is_none_or(|alias_ty| {
        matches!(ck.resolve_type_alias(alias_ty), TypeExpr::Named { name, .. } if name == "error")
    })
}

fn results_end_in_error(ck: &Checker<'_>, results: &[FnResult], type_params: &[String]) -> bool {
    results
        .last()
        .is_some_and(|r| is_builtin_error_type(ck, &r.ty, type_params))
}

fn collect_receiver_type_params(ty: &TypeExpr, out: &mut Vec<String>) {
    match ty {
        TypeExpr::Pointer { to, .. } => collect_receiver_type_params(to, out),
        TypeExpr::Named { name, .. } => {
            if let Some((_, rest)) = name.split_once('[') {
                if let Some(inner) = rest.strip_suffix(']') {
                    for part in inner.split(',') {
                        let tp = part.trim();
                        if !tp.is_empty() && tp != "_" {
                            out.push(tp.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Type-parameter names that shadow `error` on this callable. Same collection
/// as the `Decl::Fn` walk: declared params (skipping `_`), then receiver
/// params parsed out of the receiver type (`fn (h holder[error]) ...`).
fn fn_decl_type_param_names(f: &FnDecl) -> Vec<String> {
    let mut type_params = Vec::new();
    for tp in &f.type_params {
        if !tp.name.is_empty() && tp.name != "_" {
            type_params.push(tp.name.clone());
        }
    }
    if let Some(r) = &f.receiver {
        collect_receiver_type_params(&r.ty, &mut type_params);
    }
    type_params
}

fn is_e11_result_type(ck: &Checker<'_>, results: &[FnResult], http_unshadowed: bool) -> bool {
    http_unshadowed
        && results.len() == 1
        && results[0].name.is_none()
        && matches!(
            &results[0].ty,
            TypeExpr::Named { name, .. }
                if E11_PROPAGATING_TARGETS.contains(&name.as_str())
                    && !ck.type_aliases.contains_key(name.as_str())
        )
}

fn fnlit_has_valid_e11_signature(
    ck: &Checker<'_>,
    params: &[Param],
    results: &[FnResult],
    http_unshadowed: bool,
) -> bool {
    if !http_unshadowed
        || !results.is_empty()
        || params.len() != 2
        || params[0].name == "_"
        || params[0].name.is_empty()
        || params[0].name == "http"
        || params[1].name == "http"
        || params[0].name == params[1].name
    {
        return false;
    }
    let w_ok = matches!(&params[0].ty, TypeExpr::Named { name, .. } if name.is_empty())
        || matches!(ck.resolve_type_alias(&params[0].ty), TypeExpr::Named { name, .. } if name == "http.response_writer");
    let r_ok = matches!(&params[1].ty, TypeExpr::Named { name, .. } if name.is_empty())
        || matches!(
            ck.resolve_type_alias(&params[1].ty),
            TypeExpr::Pointer { to, .. } if matches!(ck.resolve_type_alias(&to), TypeExpr::Named { name, .. } if name == "http.request")
        );
    w_ok && r_ok
}

/// What the enclosing function or closure offers a `?`.
#[derive(Clone, Copy)]
enum Ctx<'a> {
    /// `main` or an error-result fn/closure.
    Ok,
    /// An E11 `http.handler` closure whose writer parameter is the first binding
    /// in `scopes[fnlit_scope_idx]` (valid only while `http` and writer stay unshadowed).
    E11Handler { fnlit_scope_idx: usize },
    /// No landing spot; the &str names the enclosing callable for the message.
    NoErrorResult(&'a str),
}

struct TryWalker<'a, 'b> {
    ck: &'b mut Checker<'a>,
    scopes: Vec<Vec<String>>,
    var_structs: Vec<HashMap<String, Option<String>>>,
    var_fns: Vec<HashMap<String, Option<(Vec<FnResult>, Vec<String>)>>>,
    type_params: Vec<String>,
    return_e11: bool,
}

impl<'a, 'b> TryWalker<'a, 'b> {
    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
        self.var_structs.push(HashMap::new());
        self.var_fns.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.var_structs.pop();
        self.var_fns.pop();
    }
    fn bind_name(&mut self, name: &str) {
        self.bind_typed(name, None, None);
    }
    fn bind_typed(
        &mut self,
        name: &str,
        struct_name: Option<String>,
        fn_sig: Option<(Vec<FnResult>, Vec<String>)>,
    ) {
        if name != "_" && !name.is_empty() {
            if let Some(top) = self.scopes.last_mut() {
                top.push(name.to_string());
            }
            if let Some(vs) = self.var_structs.last_mut() {
                vs.insert(name.to_string(), struct_name);
            }
            if let Some(vf) = self.var_fns.last_mut() {
                vf.insert(name.to_string(), fn_sig);
            }
        }
    }
    fn update_assigned(
        &mut self,
        name: &str,
        struct_name: Option<String>,
        fn_sig: Option<(Vec<FnResult>, Vec<String>)>,
    ) {
        if name == "_" || name.is_empty() {
            return;
        }
        for vs in self.var_structs.iter_mut().rev() {
            if let Some(slot) = vs.get_mut(name) {
                *slot = struct_name;
                break;
            }
        }
        for vf in self.var_fns.iter_mut().rev() {
            if let Some(slot) = vf.get_mut(name) {
                *slot = fn_sig;
                break;
            }
        }
    }
    fn is_local_bound(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.iter().any(|n| n == name))
    }
    fn lookup_var_struct(&self, name: &str) -> Option<String> {
        for vs in self.var_structs.iter().rev() {
            if let Some(entry) = vs.get(name) {
                return entry.clone();
            }
        }
        None
    }
    fn lookup_var_fn(&self, name: &str) -> Option<(Vec<FnResult>, Vec<String>)> {
        for vf in self.var_fns.iter().rev() {
            if let Some(entry) = vf.get(name) {
                return entry.clone();
            }
        }
        None
    }
    fn is_unshadowed_pkg_root(&self, pkg: &str) -> bool {
        !(self.ck.fns.contains_key(pkg)
            || self.ck.structs.contains_key(pkg)
            || self.ck.struct_type_params.contains_key(pkg)
            || self.ck.enums.contains_key(pkg)
            || self.ck.consts.contains_key(pkg)
            || self.ck.type_aliases.contains_key(pkg)
            || self.ck.file.decls.iter().any(|d| matches!(d, Decl::Const(c) if c.name == pkg))
            || self.type_params.iter().any(|tp| tp == pkg)
            || self.scopes.iter().any(|s| s.iter().any(|n| n == pkg)))
    }
    fn is_writer_shadowed_since(&self, fnlit_scope_idx: usize) -> bool {
        let Some(writer) = self.scopes.get(fnlit_scope_idx).and_then(|s| s.first()) else {
            return true;
        };
        self.scopes
            .get(fnlit_scope_idx.saturating_add(1)..)
            .is_some_and(|inner| inner.iter().any(|s| s.iter().any(|n| n == writer)))
    }
    fn is_e11_conversion_call(&self, callee: &Expr) -> bool {
        if let Expr::Field { base, name, .. } = callee {
            if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                let full = format!("{}.{}", pkg, name);
                return self.is_unshadowed_pkg_root(pkg)
                    && E11_PROPAGATING_TARGETS.contains(&full.as_str());
            }
        }
        false
    }
}

pub(crate) fn check_try_contexts<'a>(ck: &mut Checker<'a>, file: &'a File) {
    let mut w = TryWalker {
        ck,
        scopes: Vec::new(),
        var_structs: Vec::new(),
        var_fns: Vec::new(),
        type_params: Vec::new(),
        return_e11: false,
    };
    for d in &file.decls {
        match d {
            Decl::Const(c) => {
                let ctx = Ctx::NoErrorResult(c.name.as_str());
                if let Some(ty) = &c.ty {
                    walk_type(&mut w, ty, ctx);
                }
                walk_expr(&mut w, &c.value, ctx);
            }
            Decl::Struct(s) => s.fields.iter().for_each(|f| walk_type(&mut w, &f.ty, Ctx::NoErrorResult(s.name.as_str()))),
            Decl::TypeAlias(a) => walk_type(&mut w, &a.ty, Ctx::NoErrorResult(a.name.as_str())),
            Decl::Enum(e) => e.variants.iter().filter_map(|v| v.payload.as_ref()).for_each(|p| walk_type(&mut w, p, Ctx::NoErrorResult(e.name.as_str()))),
            Decl::Fn(f) => {
                w.type_params = fn_decl_type_param_names(f);
                let sig_ctx = Ctx::NoErrorResult(f.name.as_str());
                if let Some(r) = &f.receiver {
                    walk_type(&mut w, &r.ty, sig_ctx);
                }
                f.params.iter().for_each(|p| walk_type(&mut w, &p.ty, sig_ctx));
                f.results.iter().for_each(|r| walk_type(&mut w, &r.ty, sig_ctx));
                let Some(body) = &f.body else {
                    w.type_params.clear();
                    continue;
                };
                w.push_scope();
                if let Some(r) = &f.receiver {
                    let sn = resolve_struct_name(w.ck, &r.ty);
                    let fsig = resolve_fn_type_results(w.ck, &r.ty).map(|rs| (rs, w.type_params.clone()));
                    w.bind_typed(&r.name, sn, fsig);
                }
                for p in &f.params {
                    let sn = resolve_struct_name(w.ck, &p.ty);
                    let fsig = resolve_fn_type_results(w.ck, &p.ty).map(|rs| (rs, w.type_params.clone()));
                    w.bind_typed(&p.name, sn, fsig);
                }
                for r in &f.results {
                    if let Some(n) = r.name.as_deref() {
                        let sn = resolve_struct_name(w.ck, &r.ty);
                        let fsig = resolve_fn_type_results(w.ck, &r.ty).map(|rs| (rs, w.type_params.clone()));
                        w.bind_typed(n, sn, fsig);
                    }
                }
                let is_main = f.name == "main" && f.receiver.is_none();
                let ctx = if is_main || results_end_in_error(w.ck, &f.results, &w.type_params) {
                    Ctx::Ok
                } else {
                    Ctx::NoErrorResult(f.name.as_str())
                };
                let http_ok = w.is_unshadowed_pkg_root("http");
                w.return_e11 = !is_main && is_e11_result_type(w.ck, &f.results, http_ok);
                walk_block(&mut w, body, ctx);
                w.return_e11 = false;
                w.pop_scope();
                w.type_params.clear();
            }
            Decl::Import { .. } => {}
        }
    }
}

fn walk_type(ck: &mut TryWalker<'_, '_>, ty: &TypeExpr, ctx: Ctx<'_>) {
    match ty {
        TypeExpr::Named { .. } => {}
        TypeExpr::Pointer { to, .. }
        | TypeExpr::Slice { elem: to, .. }
        | TypeExpr::Chan { elem: to, .. }
        | TypeExpr::Option { elem: to, .. } => walk_type(ck, to, ctx),
        TypeExpr::Array { len, elem, .. } => {
            if let Some(l) = len {
                let len_ctx = match ctx {
                    Ctx::Ok | Ctx::E11Handler { .. } => Ctx::NoErrorResult("array length"),
                    no_err => no_err,
                };
                walk_expr(ck, l, len_ctx);
            }
            walk_type(ck, elem, ctx);
        }
        TypeExpr::Map { key, value, .. } => {
            walk_type(ck, key, ctx);
            walk_type(ck, value, ctx);
        }
        TypeExpr::Fn { params, results, .. } => {
            params.iter().for_each(|p| walk_type(ck, &p.ty, ctx));
            results.iter().for_each(|r| walk_type(ck, r, ctx));
        }
    }
}

fn walk_block(ck: &mut TryWalker<'_, '_>, b: &Block, ctx: Ctx<'_>) {
    ck.push_scope();
    for s in &b.stmts {
        walk_stmt(ck, s, ctx);
    }
    ck.pop_scope();
}

fn walk_stmt(ck: &mut TryWalker<'_, '_>, s: &Stmt, ctx: Ctx<'_>) {
    match s {
        Stmt::Local(l) => {
            let val_ctx = if l.kind == LocalKind::Const {
                Ctx::NoErrorResult(l.names.first().map(|s| s.as_str()).unwrap_or("const"))
            } else {
                ctx
            };
            if let Some(ty) = &l.ty {
                walk_type(ck, ty, val_ctx);
            }
            l.values.iter().for_each(|v| walk_expr(ck, v, val_ctx));
            let ty_sn = l.ty.as_ref().and_then(|ty| resolve_struct_name(ck.ck, ty));
            let ty_fn = l
                .ty
                .as_ref()
                .and_then(|ty| resolve_fn_type_results(ck.ck, ty))
                .map(|rs| (rs, ck.type_params.clone()));
            if l.names.len() == l.values.len() {
                let mut inferred: Vec<(Option<String>, Option<(Vec<FnResult>, Vec<String>)>)> =
                    Vec::with_capacity(l.values.len());
                for v in &l.values {
                    let sn = ty_sn.clone().or_else(|| expr_struct_name(ck, v));
                    let fsig = ty_fn.clone().or_else(|| expr_fn_sig(ck, v));
                    inferred.push((sn, fsig));
                }
                for (n, (sn, fsig)) in l.names.iter().zip(inferred) {
                    ck.bind_typed(n, sn, fsig);
                }
            } else {
                for n in &l.names {
                    ck.bind_typed(n, ty_sn.clone(), ty_fn.clone());
                }
            }
        }
        Stmt::Assign {
            targets, values, ..
        } => {
            for t in targets {
                walk_place(ck, &t.kind, ctx);
            }
            values.iter().for_each(|v| walk_expr(ck, v, ctx));
            if targets.len() == values.len() {
                let mut inferred: Vec<(Option<String>, Option<(Vec<FnResult>, Vec<String>)>)> =
                    Vec::with_capacity(values.len());
                for v in values {
                    inferred.push((expr_struct_name(ck, v), expr_fn_sig(ck, v)));
                }
                for (t, (sn, fsig)) in targets.iter().zip(inferred) {
                    if let PlaceKind::Ident(name) = &t.kind {
                        ck.update_assigned(name, sn, fsig);
                    }
                }
            } else {
                for t in targets {
                    if let PlaceKind::Ident(name) = &t.kind {
                        ck.update_assigned(name, None, None);
                    }
                }
            }
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
            ck.push_scope();
            if let Some(i) = init {
                walk_stmt(ck, i, ctx);
            }
            walk_expr(ck, cond, ctx);
            walk_block(ck, then, ctx);
            if let Some(o) = otherwise {
                walk_stmt(ck, o, ctx);
            }
            ck.pop_scope();
        }
        Stmt::For { kind, body, .. } => {
            ck.push_scope();
            match kind {
                ForKind::While { cond } => walk_expr(ck, cond, ctx),
                ForKind::Range { var, lo, hi, .. } => {
                    walk_expr(ck, lo, ctx);
                    walk_expr(ck, hi, ctx);
                    ck.bind_name(var);
                }
                ForKind::Iterate { vars, expr } => {
                    walk_expr(ck, expr, ctx);
                    vars.iter().for_each(|v| ck.bind_name(v));
                }
                ForKind::Infinite => {}
            }
            walk_block(ck, body, ctx);
            ck.pop_scope();
        }
        Stmt::Return { values, .. } => {
            if ck.return_e11
                && ck.is_unshadowed_pkg_root("http")
                && values.len() == 1
                && matches!(values[0], Expr::FnLit { .. })
            {
                walk_fnlit(ck, &values[0], true, ctx);
            } else {
                values.iter().for_each(|v| walk_expr(ck, v, ctx));
            }
        }
        Stmt::Task { callee, args, .. } => {
            walk_expr(ck, callee, ctx);
            args.iter().for_each(|a| walk_expr(ck, &a.value, ctx));
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => walk_block(ck, body, ctx),
        Stmt::Match { subject, arms, .. } => {
            walk_expr(ck, subject, ctx);
            for a in arms {
                ck.push_scope();
                if let MatchPattern::Path { binding: Some(b), .. } = &a.pattern {
                    ck.bind_name(b);
                }
                if let Some(g) = &a.guard {
                    walk_expr(ck, g, ctx);
                }
                walk_block(ck, &a.body, ctx);
                ck.pop_scope();
            }
        }
        Stmt::Select { arms, .. } => {
            for a in arms {
                ck.push_scope();
                match &a.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        walk_expr(ck, chan, ctx);
                        walk_expr(ck, value, ctx);
                    }
                    SelectArmKind::Recv { names, chan } => {
                        walk_expr(ck, chan, ctx);
                        names.iter().for_each(|n| ck.bind_name(n));
                    }
                    SelectArmKind::Default => {}
                }
                walk_block(ck, &a.body, ctx);
                ck.pop_scope();
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn walk_place(ck: &mut TryWalker<'_, '_>, p: &PlaceKind, ctx: Ctx<'_>) {
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

fn walk_fnlit(ck: &mut TryWalker<'_, '_>, e: &Expr, e11: bool, outer_ctx: Ctx<'_>) {
    if let Expr::FnLit {
        params,
        results,
        body,
        ..
    } = e
    {
        params.iter().for_each(|p| walk_type(ck, &p.ty, outer_ctx));
        results.iter().for_each(|r| walk_type(ck, &r.ty, outer_ctx));
        // E4: lexical, like `return` — the closure's OWN results decide.
        let http_before = ck.is_unshadowed_pkg_root("http");
        let saved_return_e11 = std::mem::replace(&mut ck.return_e11, false);
        ck.push_scope();
        let fnlit_scope_idx = ck.scopes.len() - 1;
        for p in params {
            let sn = resolve_struct_name(ck.ck, &p.ty);
            let fsig = resolve_fn_type_results(ck.ck, &p.ty).map(|rs| (rs, ck.type_params.clone()));
            ck.bind_typed(&p.name, sn, fsig);
        }
        for r in results {
            if let Some(n) = r.name.as_deref() {
                let sn = resolve_struct_name(ck.ck, &r.ty);
                let fsig = resolve_fn_type_results(ck.ck, &r.ty).map(|rs| (rs, ck.type_params.clone()));
                ck.bind_typed(n, sn, fsig);
            }
        }
        let ctx = if e11 {
            if fnlit_has_valid_e11_signature(ck.ck, params, results, http_before) {
                Ctx::E11Handler { fnlit_scope_idx }
            } else {
                Ctx::NoErrorResult("closure")
            }
        } else if results_end_in_error(ck.ck, results, &ck.type_params) {
            Ctx::Ok
        } else {
            Ctx::NoErrorResult("closure")
        };
        let http_ok = ck.is_unshadowed_pkg_root("http");
        ck.return_e11 = is_e11_result_type(ck.ck, results, http_ok);
        walk_block(ck, body, ctx);
        ck.pop_scope();
        ck.return_e11 = saved_return_e11;
    }
}

fn is_option_type(ck: &Checker<'_>, ty: &TypeExpr) -> bool {
    matches!(ck.resolve_type_alias(ty), TypeExpr::Option { .. })
}

fn resolve_struct_name(ck: &Checker<'_>, ty: &TypeExpr) -> Option<String> {
    let mut cur = ck.resolve_type_alias(ty);
    if let TypeExpr::Pointer { to, .. } = cur {
        cur = ck.resolve_type_alias(&to);
    }
    let TypeExpr::Named { name, .. } = cur else {
        return None;
    };
    let head = name.split('[').next().unwrap_or(name.as_str());
    if ck.structs.contains_key(head) {
        Some(head.to_string())
    } else {
        None
    }
}

fn resolve_fn_type_results(ck: &Checker<'_>, ty: &TypeExpr) -> Option<Vec<FnResult>> {
    let TypeExpr::Fn { results, .. } = ck.resolve_type_alias(ty) else {
        return None;
    };
    Some(
        results
            .into_iter()
            .map(|t| FnResult { name: None, ty: t })
            .collect(),
    )
}

fn expr_fn_sig(ck: &TryWalker<'_, '_>, e: &Expr) -> Option<(Vec<FnResult>, Vec<String>)> {
    match unwrap_parens(e) {
        Expr::Move { operand, .. } => expr_fn_sig(ck, operand),
        Expr::FnLit { results, .. } => Some((results.clone(), ck.type_params.clone())),
        Expr::Ident { name, .. } => {
            if ck.is_local_bound(name) {
                ck.lookup_var_fn(name)
            } else {
                let head = name.split('[').next().unwrap_or(name.as_str());
                ck.ck
                    .fns
                    .get(head)
                    .map(|f| (f.results.clone(), fn_decl_type_param_names(f)))
            }
        }
        _ => None,
    }
}

fn expr_struct_name(ck: &TryWalker<'_, '_>, e: &Expr) -> Option<String> {
    match unwrap_parens(e) {
        Expr::Move { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            ..
        }
        | Expr::Try { operand, .. } => expr_struct_name(ck, operand),
        Expr::Ident { name, .. } => ck.lookup_var_struct(name),
        Expr::StructLit { ty: Some(t), .. } => resolve_struct_name(ck.ck, t),
        Expr::Field { base, name, .. } => {
            let sn = expr_struct_name(ck, base)?;
            let fields = ck.ck.structs.get(sn.as_str())?;
            let (_, fty) = fields.iter().find(|(fname, _)| fname == name)?;
            resolve_struct_name(ck.ck, fty)
        }
        Expr::Call { callee, args, .. } => {
            let mut callee_head = unwrap_parens(callee);
            while let Expr::Index { base, .. } = callee_head {
                callee_head = unwrap_parens(base);
            }
            match callee_head {
                Expr::Ident { name, span } => {
                    let head = name.split('[').next().unwrap_or(name.as_str());
                    if head == "new" && !ck.is_local_bound("new") && !ck.ck.fns.contains_key("new") {
                        if let Some(arg) = args.first() {
                            if let Expr::Ident { name: tname, .. } = unwrap_parens(&arg.value) {
                                if !ck.is_local_bound(tname) {
                                    return resolve_struct_name(
                                       ck.ck,
                                       &TypeExpr::Named {
                                           name: tname.clone(),
                                           span: *span,
                                       },
                                    );
                                }
                            }
                        }
                    }
                    if ck.is_local_bound(head) {
                        let (results, _) = ck.lookup_var_fn(head)?;
                        return results
                            .first()
                            .and_then(|r| resolve_struct_name(ck.ck, &r.ty));
                    }
                    ck.ck
                        .fns
                        .get(head)
                        .and_then(|f| f.results.first())
                        .and_then(|r| resolve_struct_name(ck.ck, &r.ty))
                }
                Expr::Field { base, name, .. } => {
                    let head = name.split('[').next().unwrap_or(name.as_str());
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        if !ck.is_local_bound(pkg) {
                            if let Some(f) = ck.ck.fns.get(&format!("{pkg}.{head}")) {
                                return f
                                    .results
                                    .first()
                                    .and_then(|r| resolve_struct_name(ck.ck, &r.ty));
                            }
                        }
                    }
                    let recv_sn = expr_struct_name(ck, base)?;
                    let m = ck.ck.method_decls.get(&(recv_sn, head.to_string()))?;
                    m.results
                        .first()
                        .and_then(|r| resolve_struct_name(ck.ck, &r.ty))
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// E3/E4 operand half: `?` is postfix on an error-returning call only.
/// Non-calls (`1?`, `x.f?`, `none?`) and known calls whose results do not
/// end in `error` (including `option[T]`) are HYB-E2011. Unresolved /
/// foreign / locally-shadowed callees stay fail-open — those shapes are
/// owned by emit / other passes.
fn check_try_operand(ck: &mut TryWalker<'_, '_>, operand: &Expr, span: Span) {
    let op = unwrap_parens(operand);
    let Expr::Call { callee, .. } = op else {
        ck.ck.diags.push(
            Diagnostic::error(
                "HYB-E2011",
                ck.ck.path.clone(),
                span,
                format!(
                    "`?` applies only to an error-returning call (E3), but `{}` is not a call (HYB-E2011)",
                    op.render()
                ),
            )
            .with_notes(vec![
                "E3: `v := f(args)?` unwraps a trailing `error` result; E4: `?` on a non-call or on `option` is a type error".to_string(),
                "why: hy-emit always desugars `?` to `…, hyErr := <operand>`, so a non-call yields `assignment mismatch` / unbuildable Go (GUARANTEES Row 13)".to_string(),
                "fix: call a function whose results end in `error`, or handle the value without `?`".to_string(),
            ]),
        );
        return;
    };

    let callee = unwrap_parens(callee);
    let mut callee_head = callee;
    while let Expr::Index { base, .. } = callee_head {
        callee_head = unwrap_parens(base);
    }
    // Judge *this callee's* results with *its* type params, not the walker's.
    // A file-level fn from `ck.fns` uses `fn_decl_type_param_names` (own params
    // plus receiver params — the same list `Decl::Fn` passes). An `FnLit` has
    // no type-param list; `walk_fnlit` judges that signature with the enclosing
    // function's type params, which are in scope for the literal.
    let resolved_sig: Option<(Vec<FnResult>, Vec<String>)> = match callee_head {
        Expr::Ident { name, .. } => {
            let head = name.split('[').next().unwrap_or(name.as_str());
            if ck.is_local_bound(head) {
                ck.lookup_var_fn(head)
            } else {
                ck.ck
                    .fns
                    .get(head)
                    .map(|f| (f.results.clone(), fn_decl_type_param_names(f)))
            }
        }
        Expr::FnLit { results, .. } => Some((results.clone(), ck.type_params.clone())),
        Expr::Field { base, name, .. } => {
            let head = name.split('[').next().unwrap_or(name.as_str());
            let assoc = if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                if !ck.is_local_bound(pkg) {
                    ck.ck
                        .fns
                        .get(&format!("{pkg}.{head}"))
                        .map(|f| (f.results.clone(), fn_decl_type_param_names(f)))
                } else {
                    None
                }
            } else {
                None
            };
            assoc.or_else(|| {
                let recv_sn = expr_struct_name(ck, base)?;
                if let Some(m) = ck.ck.method_decls.get(&(recv_sn.clone(), head.to_string())) {
                    return Some((m.results.clone(), fn_decl_type_param_names(m)));
                }
                let fields = ck.ck.structs.get(recv_sn.as_str())?;
                let (_, fty) = fields.iter().find(|(fname, _)| fname == name)?;
                let mut tps = ck.type_params.clone();
                if let Some(stps) = ck.ck.struct_type_params.get(recv_sn.as_str()) {
                    tps.extend(stps.iter().map(|tp| tp.name.clone()));
                }
                resolve_fn_type_results(ck.ck, fty).map(|rs| (rs, tps))
            })
        }
        _ => None,
    };

    let Some((results, type_params)) = resolved_sig else {
        return;
    };

    if results_end_in_error(ck.ck, &results, &type_params) {
        return;
    }

    if results.len() == 1 && is_option_type(ck.ck, &results[0].ty) {
        ck.ck.diags.push(
            Diagnostic::error(
                "HYB-E2011",
                ck.ck.path.clone(),
                span,
                format!(
                    "`?` on `option` is a type error — absence and failure stay distinct (E4/T20, HYB-E2011)"
                ),
            )
            .with_notes(vec![
                "E4: `?` unwraps a trailing `error`; `option[T]` uses `match` / `.unwrap()` / `.or(...)` instead".to_string(),
                "why: hy-emit still desugars `?` as an error-pair binding, so `opt()?` fails `go build` with `assignment mismatch` (GUARANTEES Row 13)".to_string(),
                "fix: `match opt() { some(v): …; none: … }` or `opt().or(default)`".to_string(),
            ]),
        );
        return;
    }

    let label = callee.render();
    ck.ck.diags.push(
        Diagnostic::error(
            "HYB-E2011",
            ck.ck.path.clone(),
            span,
            format!(
                "`?` on `{}` whose results do not end in `error` (E3/E4, HYB-E2011)",
                label
            ),
        )
        .with_notes(vec![
            "E3: `?` unwraps a trailing `error` result from a call; E4: `?` on a non-error call is a type error".to_string(),
            "why: hy-emit desugars `x := f()?` to `x, hyErr := f()`, so a call without a trailing `error` fails `go build` with `assignment mismatch` (GUARANTEES Row 13)".to_string(),
            "fix: give the callee a trailing `error` result, or drop the `?` and handle the value directly".to_string(),
        ]),
    );
}

fn walk_expr(ck: &mut TryWalker<'_, '_>, e: &Expr, ctx: Ctx<'_>) {
    match e {
        Expr::Try { operand, span } => {
            let missing_owner = match ctx {
                Ctx::Ok => None,
                Ctx::E11Handler { fnlit_scope_idx } => {
                    if !ck.is_unshadowed_pkg_root("http")
                        || ck.is_writer_shadowed_since(fnlit_scope_idx)
                    {
                        Some("closure")
                    } else {
                        None
                    }
                }
                Ctx::NoErrorResult(owner) => Some(owner),
            };
            if let Some(owner) = missing_owner {
                let owner_text = if owner == "closure" {
                    "this closure".to_string()
                } else {
                    format!("`{}`", owner)
                };
                ck.ck.diags.push(
                    Diagnostic::error(
                        "HYB-E2011",
                        ck.ck.path.clone(),
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
            check_try_operand(ck, operand, *span);
            walk_expr(ck, operand, ctx);
        }
        Expr::FnLit { .. } => walk_fnlit(ck, e, false, ctx),
        Expr::Call { callee, args, .. } => {
            walk_expr(ck, callee, ctx);
            let e11 = ck.is_e11_conversion_call(callee)
                && args.len() == 1
                && !args[0].spread
                && args[0].name.is_none()
                && matches!(args[0].value, Expr::FnLit { .. });
            if e11 {
                walk_fnlit(ck, &args[0].value, true, ctx);
            } else {
                for a in args {
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
        Expr::StructLit { ty, fields, .. } => {
            if let Some(t) = ty {
                walk_type(ck, t, ctx);
            }
            fields.iter().for_each(|f| walk_expr(ck, &f.value, ctx));
        }
        Expr::SliceLit { elem, items, .. } => {
            if let Some(t) = elem {
                walk_type(ck, t, ctx);
            }
            items.iter().for_each(|i| walk_expr(ck, i, ctx));
        }
        Expr::MapLit { key, value, entries, .. } => {
            walk_type(ck, key, ctx);
            walk_type(ck, value, ctx);
            for (k, v) in entries {
                walk_expr(ck, k, ctx);
                walk_expr(ck, v, ctx);
            }
        }
    }
}
