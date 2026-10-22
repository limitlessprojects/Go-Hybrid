use hy_syntax::ast::*;

fn mangle_ident(name: &mut String) {
    if name == "default" {
        *name = "hyKw_default".to_string();
    } else if let Some((head, tail)) = name.split_once('[') {
        let mut m_head = head.to_string();
        mangle_ident(&mut m_head);
        let mut m_tail = String::with_capacity(tail.len());
        let mut cur = String::new();
        for ch in tail.chars() {
            if ch == '_' || ch.is_ascii_alphanumeric() {
                cur.push(ch);
            } else {
                if !cur.is_empty() {
                    mangle_ident(&mut cur);
                    m_tail.push_str(&cur);
                    cur.clear();
                }
                m_tail.push(ch);
            }
        }
        if !cur.is_empty() {
            mangle_ident(&mut cur);
            m_tail.push_str(&cur);
        }
        *name = format!("{m_head}[{m_tail}");
    } else if name.contains('.') {
        *name = name
            .split('.')
            .map(|seg| if seg == "default" { "hyKw_default" } else { seg })
            .collect::<Vec<_>>()
            .join(".");
    }
}

fn mangle_opt(opt: &mut Option<String>) {
    if let Some(n) = opt {
        mangle_ident(n);
    }
}

fn sanitize_opt_ty(opt: &mut Option<TypeExpr>) {
    if let Some(t) = opt {
        sanitize_ty(t);
    }
}

fn sanitize_opt_expr(opt: &mut Option<Box<Expr>>) {
    if let Some(e) = opt {
        sanitize_expr(e);
    }
}

fn sanitize_params_results(params: &mut [Param], results: &mut [FnResult]) {
    for p in params {
        mangle_ident(&mut p.name);
        sanitize_ty(&mut p.ty);
    }
    for r in results {
        mangle_opt(&mut r.name);
        sanitize_ty(&mut r.ty);
    }
}

pub(crate) fn sanitize_go_kw_file(file: &mut File, export_pub: bool) {
    for d in &mut file.decls {
        sanitize_decl(d, export_pub);
    }
}

fn sanitize_decl(decl: &mut Decl, export_pub: bool) {
    match decl {
        Decl::Import { .. } => {}
        Decl::Const(c) => {
            mangle_ident(&mut c.name);
            sanitize_opt_ty(&mut c.ty);
            sanitize_expr(&mut c.value);
        }
        Decl::Struct(s) => {
            mangle_ident(&mut s.name);
            s.type_params.iter_mut().for_each(|tp| mangle_ident(&mut tp.name));
            for f in &mut s.fields {
                mangle_ident(&mut f.name);
                sanitize_ty(&mut f.ty);
            }
        }
        Decl::Enum(e) => {
            mangle_ident(&mut e.name);
            for v in &mut e.variants {
                mangle_ident(&mut v.name);
                sanitize_opt_ty(&mut v.payload);
            }
        }
        Decl::TypeAlias(ta) => {
            mangle_ident(&mut ta.name);
            sanitize_ty(&mut ta.ty);
        }
        Decl::Fn(f) => {
            if !(export_pub && f.is_pub && !f.name.contains('.')) {
                mangle_ident(&mut f.name);
            }
            if let Some(r) = &mut f.receiver {
                mangle_ident(&mut r.name);
                sanitize_ty(&mut r.ty);
            }
            f.type_params.iter_mut().for_each(|tp| mangle_ident(&mut tp.name));
            sanitize_params_results(&mut f.params, &mut f.results);
            if let Some(b) = &mut f.body {
                sanitize_block(b);
            }
        }
    }
}

fn sanitize_ty(ty: &mut TypeExpr) {
    match ty {
        TypeExpr::Named { name, .. } => mangle_ident(name),
        TypeExpr::Pointer { to, .. } => sanitize_ty(to),
        TypeExpr::Slice { elem, .. } | TypeExpr::Chan { elem, .. } | TypeExpr::Option { elem, .. } => sanitize_ty(elem),
        TypeExpr::Array { len, elem, .. } => {
            sanitize_opt_expr(len);
            sanitize_ty(elem);
        }
        TypeExpr::Map { key, value, .. } => {
            sanitize_ty(key);
            sanitize_ty(value);
        }
        TypeExpr::Fn { params, results, .. } => {
            for p in params {
                mangle_ident(&mut p.name);
                sanitize_ty(&mut p.ty);
            }
            results.iter_mut().for_each(sanitize_ty);
        }
    }
}

fn sanitize_block(block: &mut Block) {
    block.stmts.iter_mut().for_each(sanitize_stmt);
}

fn sanitize_stmt(stmt: &mut Stmt) {
    match stmt {
        Stmt::Local(l) => {
            l.names.iter_mut().for_each(mangle_ident);
            sanitize_opt_ty(&mut l.ty);
            l.values.iter_mut().for_each(sanitize_expr);
        }
        Stmt::Assign { targets, values, .. } => {
            targets.iter_mut().for_each(sanitize_place);
            values.iter_mut().for_each(sanitize_expr);
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => sanitize_expr(expr),
        Stmt::Send { chan, value, .. } => {
            sanitize_expr(chan);
            sanitize_expr(value);
        }
        Stmt::If { init, cond, then, otherwise, .. } => {
            if let Some(s) = init { sanitize_stmt(s); }
            sanitize_expr(cond);
            sanitize_block(then);
            if let Some(s) = otherwise { sanitize_stmt(s); }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Infinite => {}
                ForKind::While { cond } => sanitize_expr(cond),
                ForKind::Range { var, lo, hi, .. } => {
                    mangle_ident(var);
                    sanitize_expr(lo);
                    sanitize_expr(hi);
                }
                ForKind::Iterate { vars, expr } => {
                    vars.iter_mut().for_each(mangle_ident);
                    sanitize_expr(expr);
                }
            }
            sanitize_block(body);
        }
        Stmt::Return { values, .. } => values.iter_mut().for_each(sanitize_expr),
        Stmt::Task { callee, args, .. } => {
            sanitize_expr(callee);
            args.iter_mut().for_each(sanitize_call_arg);
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => sanitize_block(body),
        Stmt::Match { subject, arms, .. } => {
            sanitize_expr(subject);
            for a in arms {
                if let MatchPattern::Path { segments, binding } = &mut a.pattern {
                    segments.iter_mut().for_each(mangle_ident);
                    mangle_opt(binding);
                }
                if let Some(g) = &mut a.guard { sanitize_expr(g); }
                sanitize_block(&mut a.body);
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Select { arms, .. } => {
            for a in arms {
                match &mut a.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        sanitize_expr(chan);
                        sanitize_expr(value);
                    }
                    SelectArmKind::Recv { names, chan } => {
                        names.iter_mut().for_each(mangle_ident);
                        sanitize_expr(chan);
                    }
                    SelectArmKind::Default => {}
                }
                sanitize_block(&mut a.body);
            }
        }
    }
}

fn sanitize_place(place: &mut Place) {
    match &mut place.kind {
        PlaceKind::Ident(n) => mangle_ident(n),
        PlaceKind::Index { base, index } => {
            sanitize_place(base);
            sanitize_expr(index);
        }
        PlaceKind::Field { base, name } => {
            sanitize_place(base);
            mangle_ident(name);
        }
        PlaceKind::AddrOf { base } => sanitize_place(base),
    }
}

fn sanitize_call_arg(arg: &mut CallArg) {
    mangle_opt(&mut arg.name);
    sanitize_expr(&mut arg.value);
}

fn sanitize_expr(expr: &mut Expr) {
    match expr {
        Expr::Ident { name, .. } => mangle_ident(name),
        Expr::Lit { .. } => {}
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p { sanitize_expr(e); }
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            sanitize_expr(lhs);
            sanitize_expr(rhs);
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Receive { chan: operand, .. }
        | Expr::Paren { inner: operand, .. } => sanitize_expr(operand),
        Expr::Index { base, index, .. } => {
            sanitize_expr(base);
            sanitize_expr(index);
        }
        Expr::Slice { base, lo, hi, .. } => {
            sanitize_expr(base);
            sanitize_opt_expr(lo);
            sanitize_opt_expr(hi);
        }
        Expr::Field { base, name, .. } => {
            sanitize_expr(base);
            mangle_ident(name);
        }
        Expr::Call { callee, args, .. } => {
            sanitize_expr(callee);
            args.iter_mut().for_each(sanitize_call_arg);
        }
        Expr::StructLit { ty, fields, .. } => {
            sanitize_opt_ty(ty);
            for f in fields {
                mangle_ident(&mut f.name);
                sanitize_expr(&mut f.value);
            }
        }
        Expr::SliceLit { elem, items, .. } => {
            if let Some(t) = elem { sanitize_ty(t); }
            items.iter_mut().for_each(sanitize_expr);
        }
        Expr::MapLit { key, value, entries, .. } => {
            sanitize_ty(key);
            sanitize_ty(value);
            for (k, v) in entries {
                sanitize_expr(k);
                sanitize_expr(v);
            }
        }
        Expr::FnLit { params, results, body, .. } => {
            sanitize_params_results(params, results);
            sanitize_block(body);
        }
    }
}
