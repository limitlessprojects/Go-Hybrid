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

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    Block, Decl, Expr, File, FnDecl, FnResult, ForKind, InterpPart, LocalKind, MatchPattern, Param,
    PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

use std::collections::HashMap;

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
    scope_tys: Vec<HashMap<String, Option<TypeExpr>>>,
    type_params: Vec<String>,
    return_e11: bool,
}

impl<'a, 'b> TryWalker<'a, 'b> {
    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
        self.scope_tys.push(HashMap::new());
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
        self.scope_tys.pop();
    }
    fn bind_name(&mut self, name: &str) {
        self.bind_typed_name(name, None);
    }
    fn bind_typed_name(&mut self, name: &str, ty: Option<TypeExpr>) {
        if name != "_" && !name.is_empty() {
            if let Some(top) = self.scopes.last_mut() {
                top.push(name.to_string());
            }
            if let Some(top_ty) = self.scope_tys.last_mut() {
                top_ty.insert(name.to_string(), ty);
            }
        }
    }
    fn lookup_local_ty(&self, name: &str) -> Option<Option<&TypeExpr>> {
        for scope in self.scope_tys.iter().rev() {
            if let Some(entry) = scope.get(name) {
                return Some(entry.as_ref());
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
        scope_tys: Vec::new(),
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
                    w.bind_typed_name(&r.name, Some(r.ty.clone()));
                }
                f.params
                    .iter()
                    .for_each(|p| w.bind_typed_name(&p.name, Some(p.ty.clone())));
                f.results
                    .iter()
                    .filter_map(|r| r.name.as_deref().map(|n| (n, r.ty.clone())))
                    .for_each(|(n, ty)| w.bind_typed_name(n, Some(ty)));
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
            if let Some(ty) = &l.ty {
                if l.names.len() == 1 {
                    ck.bind_typed_name(&l.names[0], Some(ty.clone()));
                } else {
                    l.names.iter().for_each(|n| ck.bind_name(n));
                }
            } else if l.names.len() == l.values.len() {
                let inferred: Vec<Option<TypeExpr>> = l
                    .values
                    .iter()
                    .map(|v| infer_expr_type(ck, v))
                    .collect();
                for (n, ty) in l.names.iter().zip(inferred) {
                    ck.bind_typed_name(n, ty);
                }
            } else {
                l.names.iter().for_each(|n| ck.bind_name(n));
            }
        }
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
        params
            .iter()
            .for_each(|p| ck.bind_typed_name(&p.name, Some(p.ty.clone())));
        results
            .iter()
            .filter_map(|r| r.name.as_deref().map(|n| (n, r.ty.clone())))
            .for_each(|(n, ty)| ck.bind_typed_name(n, Some(ty)));
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
    match ck.resolve_type_alias(ty) {
        TypeExpr::Option { .. } => true,
        TypeExpr::Named { name, .. } => name == "option" || name.starts_with("option["),
        _ => false,
    }
}

fn strip_type_args(name: &str) -> &str {
    name.split('[').next().unwrap_or(name)
}

fn split_explicit_type_args(name: &str) -> Vec<&str> {
    let Some((_, rest)) = name.split_once('[') else {
        return Vec::new();
    };
    let Some(inner) = rest.strip_suffix(']') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let part = inner[start..idx].trim();
                if !part.is_empty() {
                    out.push(part);
                }
                start = idx + 1;
            }
            _ => {}
        }
    }
    let tail = inner[start..].trim();
    if !tail.is_empty() {
        out.push(tail);
    }
    out
}

fn substitute_result_ty(ty: &TypeExpr, subst: &HashMap<String, TypeExpr>) -> TypeExpr {
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        TypeExpr::Named { name, span } => subst
            .get(name.as_str())
            .cloned()
            .unwrap_or_else(|| TypeExpr::Named {
                name: name.clone(),
                span: *span,
            }),
        TypeExpr::Option { elem, span } => TypeExpr::Option {
            elem: Box::new(substitute_result_ty(elem, subst)),
            span: *span,
        },
        other => other.clone(),
    }
}

fn instantiate_fn_decl_results(
    f: &FnDecl,
    type_arg_name: &str,
    span: Span,
) -> (Vec<TypeExpr>, Vec<String>) {
    let explicit = split_explicit_type_args(type_arg_name);
    let mut subst = HashMap::new();
    let mut remaining_type_params = fn_decl_type_param_names(f);
    if !explicit.is_empty() && explicit.len() == f.type_params.len() {
        for (tp, arg) in f.type_params.iter().zip(explicit.iter()) {
            let trimmed = arg.trim();
            let rep = if let Some(inner) = trimmed.strip_prefix("option[").and_then(|s| s.strip_suffix(']')) {
                TypeExpr::Option {
                    elem: Box::new(TypeExpr::Named {
                        name: inner.trim().to_string(),
                        span,
                    }),
                    span,
                }
            } else {
                TypeExpr::Named {
                    name: trimmed.to_string(),
                    span,
                }
            };
            subst.insert(tp.name.clone(), rep);
        }
        remaining_type_params.retain(|tp| !subst.contains_key(tp));
    }
    let res = f
        .results
        .iter()
        .map(|r| substitute_result_ty(&r.ty, &subst))
        .collect();
    (res, remaining_type_params)
}

fn peel_pointer_alias<'a>(ck: &'a Checker<'_>, mut ty: TypeExpr) -> TypeExpr {
    for _ in 0..8 {
        ty = ck.resolve_type_alias(&ty);
        if let TypeExpr::Pointer { to, .. } = ty {
            ty = *to;
        } else {
            break;
        }
    }
    ty
}

fn struct_field_type(ck: &Checker<'_>, struct_ty: &TypeExpr, field: &str) -> Option<TypeExpr> {
    let TypeExpr::Named { name, .. } = peel_pointer_alias(ck, struct_ty.clone()) else {
        return None;
    };
    let base = strip_type_args(&name);
    ck.structs.get(base).and_then(|fields| {
        fields
            .iter()
            .find(|(fname, _)| fname == field)
            .map(|(_, fty)| fty.clone())
    })
}

fn infer_expr_type(ck: &TryWalker<'_, '_>, expr: &Expr) -> Option<TypeExpr> {
    match unwrap_parens(expr) {
        Expr::FnLit {
            params,
            results,
            span,
            ..
        } => Some(TypeExpr::Fn {
            params: params.clone(),
            results: results.iter().map(|r| r.ty.clone()).collect(),
            span: *span,
        }),
        Expr::StructLit {
            ty: Some(ty), ..
        } => Some(ty.clone()),
        Expr::AddrOf { operand, .. } | Expr::Move { operand, .. } => infer_expr_type(ck, operand),
        Expr::Try { operand, .. } => match unwrap_parens(operand) {
            Expr::Call { callee, .. } => {
                let (res, _) = resolve_callee_results(ck, callee);
                res.and_then(|mut rs| {
                    if rs.len() >= 2 {
                        Some(rs.remove(0))
                    } else {
                        None
                    }
                })
            }
            _ => None,
        },
        Expr::Call { callee, .. } => {
            let (res, _) = resolve_callee_results(ck, callee);
            res.and_then(|rs| if rs.len() == 1 { Some(rs[0].clone()) } else { None })
        }
        Expr::Ident { name, .. } => {
            if let Some(local) = ck.lookup_local_ty(name) {
                return local.cloned();
            }
            ck.ck.consts.get(name.as_str()).and_then(|b| b.ty.clone())
        }
        Expr::Field { base, name, .. } => {
            let base_ty = infer_expr_type(ck, base)?;
            struct_field_type(ck.ck, &base_ty, strip_type_args(name))
        }
        _ => None,
    }
}

fn resolve_callee_results(
    ck: &TryWalker<'_, '_>,
    callee: &Expr,
) -> (Option<Vec<TypeExpr>>, Vec<String>) {
    let mut cur = unwrap_parens(callee);
    while let Expr::Index { base, .. } = cur {
        cur = unwrap_parens(base);
    }
    match cur {
        Expr::Ident { name, span } => {
            if let Some(bound_ty_opt) = ck.lookup_local_ty(name) {
                let Some(bound_ty) = bound_ty_opt else {
                    return (None, Vec::new());
                };
                if let TypeExpr::Fn { results, .. } = ck.ck.resolve_type_alias(bound_ty) {
                    return (Some(results), ck.type_params.clone());
                }
                return (None, Vec::new());
            }
            let base = strip_type_args(name);
            let fn_decl = ck.ck.fns.get(name.as_str()).or_else(|| ck.ck.fns.get(base));
            match fn_decl {
                Some(f) => {
                    let (res, tps) = instantiate_fn_decl_results(f, name, *span);
                    (Some(res), tps)
                }
                None => (None, Vec::new()),
            }
        }
        Expr::Field { base, name, span } => {
            let method_base = strip_type_args(name);
            if let Expr::Ident { name: root, .. } = unwrap_parens(base) {
                if ck.lookup_local_ty(root).is_none()
                    && !ck.type_params.iter().any(|tp| tp == root)
                {
                    let full_raw = format!("{}.{}", strip_type_args(root), name);
                    let full_base = format!("{}.{}", strip_type_args(root), method_base);
                    if let Some(f) = ck
                        .ck
                        .fns
                        .get(full_raw.as_str())
                        .or_else(|| ck.ck.fns.get(full_base.as_str()))
                    {
                        let (res, tps) = instantiate_fn_decl_results(f, name, *span);
                        return (Some(res), tps);
                    }
                }
            }
            if let Some(recv_ty) = infer_expr_type(ck, base) {
                let peeled = peel_pointer_alias(ck.ck, recv_ty);
                if let TypeExpr::Named {
                    name: recv_name, ..
                } = &peeled
                {
                    let recv_base = strip_type_args(recv_name);
                    if let Some(f) = ck
                        .ck
                        .method_decls
                        .get(&(recv_base.to_string(), method_base.to_string()))
                    {
                        let (res, tps) = instantiate_fn_decl_results(f, name, *span);
                        return (Some(res), tps);
                    }
                    if let Some(fty) = struct_field_type(ck.ck, &peeled, method_base) {
                        if let TypeExpr::Fn { results, .. } = ck.ck.resolve_type_alias(&fty) {
                            return (Some(results), ck.type_params.clone());
                        }
                    }
                }
            }
            (None, Vec::new())
        }
        Expr::FnLit { results, .. } => (
            Some(results.iter().map(|r| r.ty.clone()).collect()),
            ck.type_params.clone(),
        ),
        _ => (None, Vec::new()),
    }
}

/// E3/E4 operand half: `?` is postfix on an error-returning call only.
/// Non-calls (`1?`, `x.f?`, `none?`) and known calls whose results do not
/// end in `error` (including `option[T]`) are HYB-E2011. Unresolved /
/// foreign / dynamically-unknown callees stay fail-open — those shapes are
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
    let (results, type_params) = resolve_callee_results(ck, callee);

    let Some(results) = results else {
        return;
    };

    if results
        .last()
        .is_some_and(|r| is_builtin_error_type(ck.ck, r, &type_params))
    {
        return;
    }

    if results.len() == 1 && is_option_type(ck.ck, &results[0]) {
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
