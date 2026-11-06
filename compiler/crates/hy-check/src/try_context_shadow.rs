//! try_context_shadow.rs — Issue #3746 (GUARANTEES Row 13, E4/E11 HYB-E2011):
//!   1. Generic type parameters (`f.type_params` or method receiver type
//!      parameters) shadowing `error`, a type alias to `error` (e.g.
//!      `type failure = error`), or `http` / writer-request aliases.
//!   2. E11 `http.handler` closures where `http` or the writer parameter `<w>`
//!      is shadowed by a closure parameter (`fn(w, http)`, `fn(http, r)`,
//!      `fn(w, w)`) or by an active inner local/loop/pattern binding at the
//!      `Expr::Try` (`?`) site.

use std::collections::HashSet;

use crate::model::{unwrap_parens, Checker};
use hy_diag::Diagnostic;
use hy_syntax::{
    Block, Decl, Expr, FnResult, ForKind, InterpPart, LocalKind, MatchPattern, Param, PlaceKind,
    SelectArmKind, Stmt, TypeExpr,
};

const E11_PROPAGATING_TARGETS: &[&str] = &["http.handler"];

fn resolve_type_alias_unshadowed(
    ck: &Checker<'_>,
    active_type_params: &HashSet<String>,
    ty: &TypeExpr,
) -> Option<TypeExpr> {
    let mut cur = ty.clone();
    for _ in 0..16 {
        match &cur {
            TypeExpr::Named { name, .. } => {
                if active_type_params.contains(name) {
                    return None;
                }
                if let Some(target) = ck.type_aliases.get(name) {
                    cur = target.clone();
                } else {
                    return Some(cur);
                }
            }
            TypeExpr::Pointer { to, span } => {
                let inner = resolve_type_alias_unshadowed(ck, active_type_params, to)?;
                return Some(TypeExpr::Pointer {
                    to: Box::new(inner),
                    span: *span,
                });
            }
            _ => return Some(cur),
        }
    }
    Some(cur)
}

fn is_builtin_error_type(
    ck: &Checker<'_>,
    active_type_params: &HashSet<String>,
    ty: &TypeExpr,
) -> bool {
    if active_type_params.contains("error") {
        return false;
    }
    let Some(resolved) = resolve_type_alias_unshadowed(ck, active_type_params, ty) else {
        return false;
    };
    if !matches!(&resolved, TypeExpr::Named { name, .. } if name == "error")
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

fn results_end_in_error(
    ck: &Checker<'_>,
    active_type_params: &HashSet<String>,
    results: &[FnResult],
) -> bool {
    results
        .last()
        .is_some_and(|r| is_builtin_error_type(ck, active_type_params, &r.ty))
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
    active_type_params: &HashSet<String>,
    params: &[Param],
    results: &[FnResult],
) -> bool {
    if !results.is_empty() || params.len() != 2 || params[0].name == "_" {
        return false;
    }
    if params[0].name == "http"
        || params[1].name == "http"
        || params[0].name == params[1].name
    {
        return false;
    }
    let w_ok = matches!(&params[0].ty, TypeExpr::Named { name, .. } if name.is_empty())
        || resolve_type_alias_unshadowed(ck, active_type_params, &params[0].ty).is_some_and(|t| {
            matches!(t, TypeExpr::Named { name, .. } if name == "http.response_writer")
        });
    let r_ok = matches!(&params[1].ty, TypeExpr::Named { name, .. } if name.is_empty())
        || resolve_type_alias_unshadowed(ck, active_type_params, &params[1].ty).is_some_and(|t| {
            matches!(
                t,
                TypeExpr::Pointer { to, .. }
                    if matches!(*to, TypeExpr::Named { name, .. } if name == "http.request")
            )
        });
    w_ok && r_ok
}

fn collect_receiver_type_params(ck: &Checker<'_>, ty: &TypeExpr, out: &mut HashSet<String>) {
    match ty {
        TypeExpr::Named { name, .. } => {
            if let Some((base, rest)) = name.split_once('[') {
                if ck.struct_type_params.contains_key(base.trim()) {
                    if let Some((inside, _)) = rest.rsplit_once(']') {
                        for part in inside.split(',') {
                            let tp = part.trim();
                            if !tp.is_empty()
                                && tp.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                            {
                                out.insert(tp.to_string());
                            }
                        }
                    }
                }
            }
        }
        TypeExpr::Pointer { to, .. } => collect_receiver_type_params(ck, to, out),
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum Ctx<'a> {
    Ok,
    E11 {
        writer_param: &'a str,
        scope_depth: usize,
    },
    NoErrorResult(&'a str),
}

struct TryShadowWalker<'a, 'b> {
    ck: &'b mut Checker<'a>,
    scopes: Vec<Vec<String>>,
    active_type_params: HashSet<String>,
    return_e11: bool,
}

impl<'a, 'b> TryShadowWalker<'a, 'b> {
    fn push_scope(&mut self) {
        self.scopes.push(Vec::new());
    }
    fn pop_scope(&mut self) {
        self.scopes.pop();
    }
    fn bind_name(&mut self, name: &str) {
        if name != "_" && !name.is_empty() {
            if let Some(top) = self.scopes.last_mut() {
                top.push(name.to_string());
            }
        }
    }
    fn is_unshadowed_pkg_root(&self, pkg: &str) -> bool {
        !(self.active_type_params.contains(pkg)
            || self.ck.fns.contains_key(pkg)
            || self.ck.structs.contains_key(pkg)
            || self.ck.struct_type_params.contains_key(pkg)
            || self.ck.enums.contains_key(pkg)
            || self.ck.consts.contains_key(pkg)
            || self.ck.type_aliases.contains_key(pkg)
            || self
                .ck
                .file
                .decls
                .iter()
                .any(|d| matches!(d, Decl::Const(c) if c.name == pkg))
            || self.scopes.iter().any(|s| s.iter().any(|n| n == pkg)))
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
    fn emit_e2011(&mut self, span: hy_diag::Span, owner: &str) {
        if self
            .ck
            .diags
            .iter()
            .any(|d| d.code == "HYB-E2011" && d.span == span)
        {
            return;
        }
        let owner_text = if owner == "closure" {
            "this closure".to_string()
        } else {
            format!("`{}`", owner)
        };
        self.ck.diags.push(
            Diagnostic::error(
                "HYB-E2011",
                self.ck.path.clone(),
                span,
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
}

impl<'a> Checker<'a> {
    pub(crate) fn check_try_context_shadows(&mut self) {
        let file = self.file;
        let mut w = TryShadowWalker {
            ck: self,
            scopes: Vec::new(),
            active_type_params: HashSet::new(),
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
                Decl::Struct(s) => {
                    s.fields
                        .iter()
                        .for_each(|f| walk_type(&mut w, &f.ty, Ctx::NoErrorResult(s.name.as_str())));
                }
                Decl::TypeAlias(a) => {
                    walk_type(&mut w, &a.ty, Ctx::NoErrorResult(a.name.as_str()));
                }
                Decl::Enum(e) => {
                    e.variants
                        .iter()
                        .filter_map(|v| v.payload.as_ref())
                        .for_each(|p| walk_type(&mut w, p, Ctx::NoErrorResult(e.name.as_str())));
                }
                Decl::Fn(f) => {
                    w.active_type_params.clear();
                    for tp in &f.type_params {
                        w.active_type_params.insert(tp.name.clone());
                    }
                    if let Some(r) = &f.receiver {
                        collect_receiver_type_params(w.ck, &r.ty, &mut w.active_type_params);
                    }
                    let sig_ctx = Ctx::NoErrorResult(f.name.as_str());
                    if let Some(r) = &f.receiver {
                        walk_type(&mut w, &r.ty, sig_ctx);
                    }
                    f.params.iter().for_each(|p| walk_type(&mut w, &p.ty, sig_ctx));
                    f.results.iter().for_each(|r| walk_type(&mut w, &r.ty, sig_ctx));
                    let Some(body) = &f.body else {
                        w.active_type_params.clear();
                        continue;
                    };
                    w.push_scope();
                    if let Some(r) = &f.receiver {
                        w.bind_name(&r.name);
                    }
                    f.params.iter().for_each(|p| w.bind_name(&p.name));
                    f.results
                        .iter()
                        .filter_map(|r| r.name.as_deref())
                        .for_each(|n| w.bind_name(n));
                    let is_main =
                        f.name == "main" && f.receiver.is_none() && f.type_params.is_empty();
                    let ctx = if is_main
                        || results_end_in_error(w.ck, &w.active_type_params, &f.results)
                    {
                        Ctx::Ok
                    } else {
                        Ctx::NoErrorResult(f.name.as_str())
                    };
                    let http_ok = w.is_unshadowed_pkg_root("http");
                    w.return_e11 = !is_main && is_e11_result_type(w.ck, &f.results, http_ok);
                    walk_block(&mut w, body, ctx);
                    w.return_e11 = false;
                    w.pop_scope();
                    w.active_type_params.clear();
                }
                Decl::Import { .. } => {}
            }
        }
    }
}

fn walk_type<'a>(ck: &mut TryShadowWalker<'_, '_>, ty: &'a TypeExpr, ctx: Ctx<'a>) {
    match ty {
        TypeExpr::Named { .. } => {}
        TypeExpr::Pointer { to, .. }
        | TypeExpr::Slice { elem: to, .. }
        | TypeExpr::Chan { elem: to, .. }
        | TypeExpr::Option { elem: to, .. } => walk_type(ck, to, ctx),
        TypeExpr::Array { len, elem, .. } => {
            if let Some(l) = len {
                let len_ctx = match ctx {
                    Ctx::Ok | Ctx::E11 { .. } => Ctx::NoErrorResult("array length"),
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
        TypeExpr::Fn {
            params, results, ..
        } => {
            params.iter().for_each(|p| walk_type(ck, &p.ty, ctx));
            results.iter().for_each(|r| walk_type(ck, r, ctx));
        }
    }
}

fn walk_block<'a>(ck: &mut TryShadowWalker<'_, '_>, b: &'a Block, ctx: Ctx<'a>) {
    ck.push_scope();
    for s in &b.stmts {
        walk_stmt(ck, s, ctx);
    }
    ck.pop_scope();
}

fn walk_stmt<'a>(ck: &mut TryShadowWalker<'_, '_>, s: &'a Stmt, ctx: Ctx<'a>) {
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
            l.names.iter().for_each(|n| ck.bind_name(n));
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
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            walk_block(ck, body, ctx)
        }
        Stmt::Match { subject, arms, .. } => {
            walk_expr(ck, subject, ctx);
            for a in arms {
                ck.push_scope();
                if let MatchPattern::Path {
                    binding: Some(b), ..
                } = &a.pattern
                {
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

fn walk_place<'a>(ck: &mut TryShadowWalker<'_, '_>, p: &'a PlaceKind, ctx: Ctx<'a>) {
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

fn walk_fnlit<'a>(
    ck: &mut TryShadowWalker<'_, '_>,
    e: &'a Expr,
    e11: bool,
    outer_ctx: Ctx<'a>,
) {
    if let Expr::FnLit {
        params,
        results,
        body,
        ..
    } = e
    {
        params.iter().for_each(|p| walk_type(ck, &p.ty, outer_ctx));
        results.iter().for_each(|r| walk_type(ck, &r.ty, outer_ctx));
        let outer_http_ok = ck.is_unshadowed_pkg_root("http");
        let e11_valid = e11
            && outer_http_ok
            && fnlit_has_valid_e11_signature(ck.ck, &ck.active_type_params, params, results);
        let err_valid =
            !e11 && results_end_in_error(ck.ck, &ck.active_type_params, results);
        let saved_return_e11 = std::mem::replace(&mut ck.return_e11, false);
        ck.push_scope();
        params.iter().for_each(|p| ck.bind_name(&p.name));
        results
            .iter()
            .filter_map(|r| r.name.as_deref())
            .for_each(|n| ck.bind_name(n));
        let ctx = if e11_valid {
            Ctx::E11 {
                writer_param: params[0].name.as_str(),
                scope_depth: ck.scopes.len(),
            }
        } else if err_valid {
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

fn walk_expr<'a>(ck: &mut TryShadowWalker<'_, '_>, e: &'a Expr, ctx: Ctx<'a>) {
    match e {
        Expr::Try { operand, span } => {
            match ctx {
                Ctx::NoErrorResult(owner) => ck.emit_e2011(*span, owner),
                Ctx::E11 {
                    writer_param,
                    scope_depth,
                } => {
                    let shadowed = ck.scopes[scope_depth..]
                        .iter()
                        .any(|s| s.iter().any(|n| n == writer_param || n == "http"));
                    if shadowed {
                        ck.emit_e2011(*span, "closure");
                    }
                }
                Ctx::Ok => {}
            }
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
        Expr::MapLit {
            key,
            value,
            entries,
            ..
        } => {
            walk_type(ck, key, ctx);
            walk_type(ck, value, ctx);
            for (k, v) in entries {
                walk_expr(ck, k, ctx);
                walk_expr(ck, v, ctx);
            }
        }
    }
}
