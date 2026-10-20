//! tuple.rs — T23: a multi-value result is not a tuple (HYB-E2002).
//!
//! SPEC-LANGUAGE.md §2.8 (T23) pins `p := minmax(3, 9)` as a compile-fail
//! claim (`// reject: HYB-E2002`), and diagnostics.json registers the code,
//! but nothing raised it: the checker accepted the program with
//! 0 diagnostics and `go build` then failed with `assignment mismatch:
//! 1 variable but minmax returns 2 values`. Accepted programs must emit
//! buildable Go, so the binding is rejected here, before emission.
//!
//! Scope:
//! a define / `let` / `var` / plain `=` assignment whose right-hand side
//! contains a call declaring two or more results, with a target count that
//! differs from the result count. Covers direct top-level fn calls, fn-value
//! bindings / aliases (including type-aliased `fn` types), declared and
//! inline closures, user-defined struct methods, indirect `fn`-typed callees
//! (struct fields, array/slice/map elements, higher-order calls), and curated
//! foreign package calls. `?`-unwrapped calls stay permissive (unwrapping the
//! error produces a single value).

use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

fn unwrap_parens_and_move(mut e: &Expr) -> &Expr {
    loop {
        match e {
            Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. } => e = inner,
            _ => return e,
        }
    }
}

impl<'a> Checker<'a> {
    /// Pass-B entry (and closure-body entry in `closures.rs`), called once
    /// per statement.
    pub(crate) fn check_stmt_multi_value(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) if l.values.len() == 1 => {
                self.check_multi_value_bind(st, &l.values[0], l.names.len(), l.span);
                self.check_subexprs_single_value(st, &l.values[0]);
            }
            Stmt::Local(l) if l.values.len() > 1 => {
                for v in &l.values {
                    self.check_expr_single_value(st, v, l.span);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.check_place_single_value(st, t);
                }
                if *op == AssignOp::Assign && values.len() == 1 {
                    self.check_multi_value_bind(st, &values[0], targets.len(), *span);
                    self.check_subexprs_single_value(st, &values[0]);
                } else {
                    for v in values {
                        self.check_expr_single_value(st, v, *span);
                    }
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.check_subexprs_single_value(st, expr);
            }
            Stmt::Send {
                chan, value, span, ..
            } => {
                self.check_expr_single_value(st, chan, chan.span());
                self.check_expr_single_value(st, value, *span);
            }
            Stmt::If { cond, .. } => {
                self.check_expr_single_value(st, cond, cond.span());
            }
            Stmt::For { kind, .. } => match kind {
                ForKind::Infinite => {}
                ForKind::While { cond } => {
                    self.check_expr_single_value(st, cond, cond.span());
                }
                ForKind::Range { lo, hi, .. } => {
                    self.check_expr_single_value(st, lo, lo.span());
                    self.check_expr_single_value(st, hi, hi.span());
                }
                ForKind::Iterate { expr, .. } => {
                    self.check_expr_single_value(st, expr, expr.span());
                }
            },
            Stmt::Return { values, .. } => {
                if values.len() == 1 {
                    self.check_subexprs_single_value(st, &values[0]);
                } else {
                    for v in values {
                        self.check_expr_single_value(st, v, v.span());
                    }
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_single_value(st, callee, callee.span());
                if args.len() == 1 && !args[0].spread {
                    self.check_subexprs_single_value(st, &args[0].value);
                } else {
                    for a in args {
                        self.check_expr_single_value(st, &a.value, a.span);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_single_value(st, subject, subject.span());
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_single_value(st, g, g.span());
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_single_value(st, chan, chan.span());
                            self.check_expr_single_value(st, value, value.span());
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_single_value(st, chan, chan.span());
                        }
                        SelectArmKind::Default => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn check_place_single_value(&mut self, st: &FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_single_value(st, base);
                self.check_expr_single_value(st, index, index.span());
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_single_value(st, base);
            }
        }
    }

    fn check_expr_single_value(&mut self, st: &FnState, expr: &Expr, span: Span) {
        self.check_multi_value_bind(st, expr, 1, span);
        self.check_subexprs_single_value(st, expr);
    }

    fn check_subexprs_single_value(&mut self, st: &FnState, expr: &Expr) {
        match unwrap_parens_and_move(expr) {
            Expr::Ident { .. } | Expr::Lit { .. } | Expr::FnLit { .. } => {}
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_single_value(st, lhs, lhs.span());
                self.check_expr_single_value(st, rhs, rhs.span());
            }
            Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } => {
                self.check_expr_single_value(st, operand, operand.span());
            }
            Expr::Try { operand, .. } => {
                self.check_subexprs_single_value(st, operand);
            }
            Expr::Receive { chan, .. } => {
                self.check_expr_single_value(st, chan, chan.span());
            }
            Expr::Index { base, index, .. } => {
                self.check_expr_single_value(st, base, base.span());
                self.check_expr_single_value(st, index, index.span());
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_single_value(st, base, base.span());
                if let Some(l) = lo {
                    self.check_expr_single_value(st, l, l.span());
                }
                if let Some(h) = hi {
                    self.check_expr_single_value(st, h, h.span());
                }
            }
            Expr::Field { base, .. } => {
                self.check_expr_single_value(st, base, base.span());
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_single_value(st, callee, callee.span());
                if args.len() == 1 && !args[0].spread {
                    self.check_subexprs_single_value(st, &args[0].value);
                } else {
                    for a in args {
                        self.check_expr_single_value(st, &a.value, a.span);
                    }
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_single_value(st, &f.value, f.span);
                }
            }
            Expr::SliceLit { items, .. } => {
                for item in items {
                    self.check_expr_single_value(st, item, item.span());
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_single_value(st, k, k.span());
                    self.check_expr_single_value(st, v, v.span());
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.check_expr_single_value(st, x, x.span());
                    }
                }
            }
            Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. } => {
                self.check_subexprs_single_value(st, inner);
            }
        }
    }

    fn check_multi_value_bind(&mut self, st: &FnState, value: &Expr, targets: usize, span: Span) {
        let Some((name, results)) = self.static_result_count(st, value) else {
            return;
        };
        if results < 2 || targets == results {
            return;
        }
        let msg = format!(
            "`{name}` returns {results} values, but {targets} {} bound: a multi-value result is not a tuple (T23)",
            if targets == 1 { "name is" } else { "names are" }
        );
        self.diags.push(
            Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                "why: Hybrid has no tuple type, and Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
            ]),
        );
    }

    /// `Some((callee, result count))` when `expr` is a direct or indirect call
    /// with a statically known result count (excluding `?`-unwrapped calls).
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        let Expr::Call { callee, .. } = unwrap_parens_and_move(expr) else {
            return None;
        };
        match unwrap_parens_and_move(callee) {
            Expr::Ident { name, .. } => {
                // 1. Local bindings (fn values, closures, parameters, aliases)
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                            return Some((name.clone(), results.len()));
                        }
                    }
                    let root = crate::freeze::resolve_alias(st, name);
                    if root != *name {
                        if let Some(rb) = self.lookup_binding(st, &root) {
                            if let Some(ty) = &rb.ty {
                                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                                    return Some((name.clone(), results.len()));
                                }
                            }
                        }
                        let rhead = Self::base_named_type_str(&root);
                        if let Some(decl) =
                            self.fns.get(root.as_str()).or_else(|| self.fns.get(rhead))
                        {
                            return Some((name.clone(), decl.results.len()));
                        }
                    }
                    if let Some(ty) = self.infer_local_fn_binding_ty(st, name, expr.span().start) {
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&ty) {
                            return Some((name.clone(), results.len()));
                        }
                    }
                    return None;
                }
                // 2. Direct call to a file-local top-level function
                let head = Self::base_named_type_str(name);
                let decl = self.fns.get(name.as_str()).or_else(|| self.fns.get(head))?;
                Some((name.clone(), decl.results.len()))
            }
            Expr::FnLit { results, .. } => Some(("fn".to_string(), results.len())),
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                let head = Self::base_named_type_str(method_name);
                // 1. Unshadowed package call (`strconv.parse_int(...)`, etc.)
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() {
                        let full = format!("{pkg}.{head}");
                        if let Some(decl) = self.fns.get(full.as_str()) {
                            return Some((format!("{pkg}.{method_name}"), decl.results.len()));
                        }
                        if let Some(count) = foreign::package_result_count(pkg.as_str(), head) {
                            return Some((format!("{pkg}.{method_name}"), count));
                        }
                    }
                }
                // 2. User-defined struct method (`b.pair()`)
                let recv_base = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match self.shape_of_expr(st, base) {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(&s).to_string())
                        }
                        TypeShape::Pointer(inner) => match *inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(&s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                if let Some(rb) = recv_base {
                    if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                        return Some((format!("{}.{method_name}", base.render()), decl.results.len()));
                    }
                }
                // 3. Struct field holding a fn value (`h.mm(3, 9)`)
                let callee_ty = self
                    .tuple_callee_ty(st, callee)
                    .or_else(|| self.ty_of_expr(st, callee))?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            other => {
                let callee_ty = self
                    .tuple_callee_ty(st, other)
                    .or_else(|| self.ty_of_expr(st, other))?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
        }
    }

    fn tuple_callee_ty(&self, st: &FnState, expr: &Expr) -> Option<TypeExpr> {
        match unwrap_parens_and_move(expr) {
            Expr::Ident { name, span } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        return Some(ty.clone());
                    }
                    return self.infer_local_fn_binding_ty(st, name, span.start);
                }
                let head = Self::base_named_type_str(name);
                let decl = self.fns.get(name.as_str()).or_else(|| self.fns.get(head))?;
                Some(TypeExpr::Fn {
                    params: decl.params.clone(),
                    results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                    span: *span,
                })
            }
            Expr::Receive { chan, .. } => {
                let cty = self
                    .tuple_callee_ty(st, chan)
                    .or_else(|| self.ty_of_expr(st, chan))?;
                match self.resolve_type_alias(&cty) {
                    TypeExpr::Chan { elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::Index { base, span, .. } => {
                match unwrap_parens_and_move(base) {
                    Expr::Ident { name, .. } if self.lookup_binding(st, name).is_none() => {
                        let head = Self::base_named_type_str(name);
                        if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                            return Some(TypeExpr::Fn {
                                params: decl.params.clone(),
                                results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                                span: *span,
                            });
                        }
                    }
                    Expr::Field {
                        base: fbase,
                        name: method_name,
                        ..
                    } => {
                        let head = Self::base_named_type_str(method_name);
                        if let Some(rb) = self.receiver_base_name(st, fbase) {
                            if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                                return Some(TypeExpr::Fn {
                                    params: decl.params.clone(),
                                    results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                                    span: *span,
                                });
                            }
                        }
                    }
                    _ => {}
                }
                let bty = self
                    .tuple_callee_ty(st, base)
                    .or_else(|| self.ty_of_expr(st, base))?;
                match self.resolve_type_alias(&bty) {
                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => Some(*elem),
                    TypeExpr::Map { value, .. } => Some(*value),
                    _ => None,
                }
            }
            Expr::Field { base, name, span } => {
                let head = Self::base_named_type_str(name);
                if let Some(rb) = self.receiver_base_name(st, base) {
                    if let Some(decl) = self.method_decls.get(&(rb.clone(), head.to_string())) {
                        return Some(TypeExpr::Fn {
                            params: decl.params.clone(),
                            results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                            span: *span,
                        });
                    }
                }
                let base_ty = self
                    .tuple_callee_ty(st, base)
                    .or_else(|| self.ty_of_expr(st, base))?;
                let resolved = self.resolve_type_alias(&base_ty);
                let target = match resolved {
                    TypeExpr::Pointer { to, .. } => self.resolve_type_alias(&to),
                    other => other,
                };
                if let TypeExpr::Named { name: raw_s, span: s_span } = target {
                    let s_head = Self::base_named_type_str(&raw_s);
                    let (_, field_ty) = self
                        .structs
                        .get(s_head)?
                        .iter()
                        .find(|(n, _)| n == name)?;
                    if let (Some(tparams), Some(i)) = (self.struct_type_params.get(s_head), raw_s.find('[')) {
                        if raw_s.ends_with(']') {
                            let inner = &raw_s[i + 1..raw_s.len() - 1];
                            let args: Vec<&str> = inner.split(',').map(|s| s.trim()).collect();
                            if args.len() == tparams.len() {
                                return Some(subst_fn_field_ty(field_ty, tparams, &args, s_span));
                            }
                        }
                    }
                    return Some(field_ty.clone());
                }
                self.ty_of_expr(st, expr)
            }
            other => self.ty_of_expr(st, other),
        }
    }

    fn receiver_base_name(&self, st: &FnState, base: &Expr) -> Option<String> {
        self.ty_of_expr(st, base)
            .map(|t| self.resolve_type_alias(&t))
            .and_then(|t| Self::base_named_type(&t))
            .or_else(|| match self.shape_of_expr(st, base) {
                TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(&s).to_string()),
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(&s).to_string()),
                    _ => None,
                },
                _ => None,
            })
    }

    fn infer_local_fn_binding_ty(
        &self,
        st: &FnState,
        target: &str,
        use_pos: hy_diag::Pos,
    ) -> Option<TypeExpr> {
        for d in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = d {
                if let Some(ty) = self.scan_block_fn_binding(&f.body, st, target, use_pos) {
                    return Some(ty);
                }
            }
        }
        None
    }

    fn scan_block_fn_binding(
        &self,
        b: &hy_syntax::Block,
        st: &FnState,
        target: &str,
        use_pos: hy_diag::Pos,
    ) -> Option<TypeExpr> {
        let mut found = None;
        for s in &b.stmts {
            if let Some(ty) = self.scan_stmt_fn_binding(s, st, target, use_pos) {
                found = Some(ty);
            }
        }
        found
    }

    fn scan_stmt_fn_binding(
        &self,
        s: &Stmt,
        st: &FnState,
        target: &str,
        use_pos: hy_diag::Pos,
    ) -> Option<TypeExpr> {
        match s {
            Stmt::Local(l) => {
                if (l.span.start.line, l.span.start.col) <= (use_pos.line, use_pos.col)
                    && l.names.len() == l.values.len()
                {
                    for (n, v) in l.names.iter().zip(l.values.iter()) {
                        if n == target {
                            return self.tuple_callee_ty(st, v);
                        }
                    }
                }
                None
            }
            Stmt::For { kind, body, span, .. } => {
                let mut res = None;
                if (span.start.line, span.start.col) <= (use_pos.line, use_pos.col) {
                    if let hy_syntax::ForKind::Iterate { vars, expr } = kind {
                        let cty = self
                            .tuple_callee_ty(st, expr)
                            .or_else(|| self.ty_of_expr(st, expr));
                        if let Some(cty) = cty {
                            let resolved = self.resolve_type_alias(&cty);
                            if vars.len() == 1 && vars[0] == target {
                                if let TypeExpr::Chan { elem, .. }
                                | TypeExpr::Slice { elem, .. }
                                | TypeExpr::Array { elem, .. } = resolved
                                {
                                    res = Some(*elem);
                                }
                            } else if vars.len() == 2 && vars[1] == target {
                                match resolved {
                                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                                        res = Some(*elem);
                                    }
                                    TypeExpr::Map { value, .. } => {
                                        res = Some(*value);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                self.scan_block_fn_binding(body, st, target, use_pos).or(res)
            }
            Stmt::Select { arms, .. } => {
                let mut res = None;
                for arm in arms {
                    if let hy_syntax::SelectArmKind::Recv { names, chan } = &arm.kind {
                        if names.first().is_some_and(|n| n == target) {
                            if let Some(cty) = self
                                .tuple_callee_ty(st, chan)
                                .or_else(|| self.ty_of_expr(st, chan))
                            {
                                if let TypeExpr::Chan { elem, .. } = self.resolve_type_alias(&cty) {
                                    res = Some(*elem);
                                }
                            }
                        }
                    }
                    if let Some(inner) = self.scan_block_fn_binding(&arm.body, st, target, use_pos)
                    {
                        res = Some(inner);
                    }
                }
                res
            }
            Stmt::If { init, then, otherwise, .. } => {
                let mut res = init
                    .as_deref()
                    .and_then(|i| self.scan_stmt_fn_binding(i, st, target, use_pos));
                if let Some(t) = self.scan_block_fn_binding(then, st, target, use_pos) {
                    res = Some(t);
                }
                if let Some(o) = otherwise
                    .as_deref()
                    .and_then(|o| self.scan_stmt_fn_binding(o, st, target, use_pos))
                {
                    res = Some(t_or(o));
                }
                res
            }
            Stmt::Managed { body, .. } => self.scan_block_fn_binding(body, st, target, use_pos),
            Stmt::Block { block, .. } => self.scan_block_fn_binding(block, st, target, use_pos),
            _ => None,
        }
    }
}

fn t_or(t: TypeExpr) -> TypeExpr {
    t
}

fn subst_fn_field_ty(
    ty: &TypeExpr,
    tparams: &[hy_syntax::TypeParam],
    args: &[&str],
    span: Span,
) -> TypeExpr {
    match ty {
        TypeExpr::Named { name, .. } => {
            for (tp, arg) in tparams.iter().zip(args.iter()) {
                if tp.name == *name {
                    return TypeExpr::Named {
                        name: (*arg).to_string(),
                        span,
                    };
                }
            }
            ty.clone()
        }
        TypeExpr::Fn {
            params,
            results,
            span: fspan,
        } => TypeExpr::Fn {
            params: params
                .iter()
                .map(|p| hy_syntax::Param {
                    name: p.name.clone(),
                    ty: subst_fn_field_ty(&p.ty, tparams, args, span),
                    span: p.span,
                })
                .collect(),
            results: results
                .iter()
                .map(|r| subst_fn_field_ty(r, tparams, args, span))
                .collect(),
            span: *fspan,
        },
        other => other.clone(),
    }
}

