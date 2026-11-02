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
use crate::model::{unwrap_parens, Binding, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Block, Expr, FnResult, ForKind, InterpPart, LocalKind, Param, Place, PlaceKind,
    SelectArmKind, Stmt, TypeExpr,
};

/// Statically known value count of a non-call binding source (T23).
pub(crate) enum BindSourceBound {
    /// A channel receive / map index: `v` or `v, ok`.
    AtMost(usize),
    /// An index of a known slice/array/string base: exactly one value.
    Exact(usize),
}

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
            Stmt::If { init, cond, .. } => {
                let mut tmp = st.clone();
                if let Some(Stmt::Local(l)) = init.as_deref() {
                    tmp.scope_push();
                    let tys = if l.values.len() == 1 && l.names.len() > 1 {
                        self.multi_value_tys(st, &l.values[0], l.names.len())
                    } else {
                        l.values.iter().map(|v| self.ty_of_expr(st, v)).collect()
                    };
                    for (i, n) in l.names.iter().enumerate() {
                        let ty = l.ty.clone().or_else(|| tys.get(i).cloned().flatten());
                        let s = ty.as_ref().map(|t| self.shape_of_type(t)).unwrap_or(TypeShape::Unknown);
                        self.define_binding(&mut tmp, n, s, ty, false, false, false);
                    }
                }
                self.check_expr_single_value(&tmp, cond, cond.span());
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
                    if st.result_shapes.len() == 1 && !self.buildable_owns_multi_value_call(st, &values[0]) {
                        self.check_multi_value_bind(st, &values[0], 1, values[0].span());
                    }
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
                    let forwarded = self.forwarded_tuple_arity(st, callee, &args[0].value);
                    // usize is Copy, so take the count rather than the tuple:
                    // the tuple carries a String and would be moved below.
                    let result_count = self
                        .static_result_count(st, &args[0].value)
                        .map(|(_, r)| r);
                    if let (Some(w), Some(r)) = (forwarded, result_count) {
                        if r >= 2 {
                            self.check_multi_value_bind(st, &args[0].value, w, args[0].span);
                        }
                    }
                    // Even when the lone argument legally forwards its multi-value
                    // result into the callee, its own subexpressions (callee and
                    // inner arguments) remain single-value contexts.
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
                        let mut arm_st = st.clone();
                        if let hy_syntax::MatchPattern::Path { segments, binding: Some(b) } = &arm.pattern {
                            if segments == &["some"] {
                                arm_st.scope_push();
                                self.bind_match_arm(&mut arm_st, b, subject, false);
                                if let Some(TypeExpr::Option { elem, .. }) = self.ty_of_expr(st, subject).map(|t| self.resolve_type_alias(&t)) {
                                    if let Some(bind) = arm_st.env_lookup_mut(b) {
                                        bind.ty = Some(*elem);
                                    }
                                }
                            }
                        }
                        self.check_expr_single_value(&arm_st, g, g.span());
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
                        SelectArmKind::Recv { names, chan } => {
                            self.check_expr_single_value(st, chan, chan.span());
                            // A receive yields at most two values (`v, ok`);
                            // 3+ names crash the emitter's select lowering —
                            // reject fail-closed as T23 before emission.
                            if names.len() > 2 {
                                let msg = format!(
                                    "select receive binds {} names, but a receive yields at most 2 values (`v` or `v, ok`) (T23)",
                                    names.len()
                                );
                                self.diags.push(
                                    Diagnostic::error("HYB-E2002", self.path.clone(), chan.span(), msg)
                                        .with_notes(vec![
                                            "why: the emitter refuses select receive with 3+ names, so the program would not build".to_string(),
                                            "fix: bind at most two names — `v := <-ch` or `v, ok := <-ch`".to_string(),
                                        ]),
                                );
                            }
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
                    let forwarded = self.forwarded_tuple_arity(st, callee, &args[0].value);
                    // usize is Copy, so take the count rather than the tuple:
                    // the tuple carries a String and would be moved below.
                    let result_count = self
                        .static_result_count(st, &args[0].value)
                        .map(|(_, r)| r);
                    if let (Some(w), Some(r)) = (forwarded, result_count) {
                        if r >= 2 {
                            self.check_multi_value_bind(st, &args[0].value, w, args[0].span);
                        }
                    }
                    // Even when the lone argument legally forwards its multi-value
                    // result into the callee, its own subexpressions (callee and
                    // inner arguments) remain single-value contexts.
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
            // Non-call binding sources: a channel receive and a map index
            // yield at most two values (`v` or `v, ok`); indexing a slice,
            // array, or string yields exactly one and has no comma-ok form.
            // Binding a count the source cannot yield fails go build with
            // `assignment mismatch` — reject fail-closed as T23.
            if let Some(bound) = self.static_bind_source_bound(st, value) {
                let (desc, over) = match bound {
                    BindSourceBound::AtMost(n) => (
                        format!("yields at most {n} values (`v` or `v, ok`)"),
                        targets > n,
                    ),
                    BindSourceBound::Exact(n) => (
                        format!("yields exactly {n} value (no comma-ok form)"),
                        targets != n,
                    ),
                };
                if over {
                    let msg = format!(
                        "`{}` {desc}, but {targets} {} bound: the binding cannot yield that many values (T23)",
                        value.render(),
                        if targets == 1 { "name is" } else { "names are" }
                    );
                    self.diags.push(
                        Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                            "why: Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
                            "fix: bind the count the source yields — `v` for an index, `v` or `v, ok` for a receive or map lookup".to_string(),
                        ]),
                    );
                }
            }
            return;
        };
        if results >= 2 {
            if targets == results {
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
            return;
        }
        // results == 1: a 1-result source bound to 2+ names is a plain
        // assignment mismatch — Go refuses `assignment mismatch: N
        // variables but name returns 1 value`. Void sources (0 results)
        // stay owned by the void-position rule.
        if targets > 1 {
            let msg = format!(
                "`{name}` returns 1 value, but {targets} names are bound: assignment mismatch (T23 binding count)"
            );
            self.diags.push(
                Diagnostic::error("HYB-E1002", self.path.clone(), span, msg).with_notes(vec![
                    "why: Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
                    "fix: bind one name, or call a function that returns one value per target".to_string(),
                ]),
            );
        }
    }

    /// The statically known value count of a non-call binding source:
    /// a receive and a map index yield at most two values, an index of a
    /// known slice/array/string base yields exactly one. `None` fails
    /// open (unresolved bases, other shapes).
    fn static_bind_source_bound(&self, st: &FnState, expr: &Expr) -> Option<BindSourceBound> {
        match unwrap_parens_and_move(expr) {
            Expr::Receive { .. } => Some(BindSourceBound::AtMost(2)),
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(..) => Some(BindSourceBound::AtMost(2)),
                TypeShape::Slice(..) | TypeShape::Array(..) | TypeShape::Str => {
                    Some(BindSourceBound::Exact(1))
                }
                // a pointer to an array auto-derefs on indexing in Go —
                // still exactly one value; every other pointer/base stays
                // fail-open.
                TypeShape::Pointer(inner) => match inner.as_ref() {
                    TypeShape::Array(..) => Some(BindSourceBound::Exact(1)),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }

    fn forwarded_tuple_arity(&self, st: &FnState, callee: &Expr, arg: &Expr) -> Option<usize> {
        match unwrap_parens_and_move(callee) {
            Expr::Ident { name, .. } => {
                if self.lookup_binding(st, name).is_some() {
                    if let Some(TypeExpr::Fn { params, .. }) = self.resolve_fn_callee_ty(st, callee)
                    {
                        return Some(params.len());
                    }
                    return None;
                }
                let head = Self::base_named_type_str(name);
                // A named user function is already owned by buildable's
                // check_multi_value_arg_arity (E1002) when buildable resolves
                // `callee` and `arg`. When `callee` is wrapped in `move` or
                // `arg` is wrapped in `move`, is a foreign package call, or
                // is an indirect call buildable does not resolve, check here.
                if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                    let callee_is_bare_ident = matches!(unwrap_parens(callee), Expr::Ident { .. });
                    if callee_is_bare_ident && self.buildable_owns_multi_value_call(st, arg) {
                        return None;
                    }
                    return Some(decl.params.len());
                }
                (name == "some").then_some(1)
            }
            Expr::FnLit { params, .. } => Some(params.len()),
            Expr::Field { base, name, .. } => {
                if matches!(name.as_str(), "insert" | "contains" | "remove")
                    && self
                        .ty_of_expr(st, base)
                        .map(|t| self.resolve_type_alias(&t))
                        .is_some_and(|t| matches!(t, TypeExpr::Named { name: n, .. } if Self::base_named_type_str(&n) == "set"))
                {
                    return Some(1);
                }
                let head = Self::base_named_type_str(name);
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
                        return Some(decl.params.len());
                    }
                }
                if let Some(TypeExpr::Fn { params, .. }) = self.resolve_fn_callee_ty(st, callee) {
                    return Some(params.len());
                }
                None
            }
            other => match self.resolve_fn_callee_ty(st, other)? {
                TypeExpr::Fn { params, .. } => Some(params.len()),
                _ => None,
            },
        }
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
                if self.lookup_binding(st, name).is_some() {
                    if let Some(TypeExpr::Fn { results, .. }) =
                        self.resolve_fn_callee_ty(st, callee)
                    {
                        return Some((name.clone(), results.len()));
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
                if let Expr::Ident { name: pkg, .. } = unwrap_parens_and_move(base) {
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
                        return Some((
                            format!("{}.{method_name}", base.render()),
                            decl.results.len(),
                        ));
                    }
                }
                // 3. Struct field holding a fn value (`h.mm(3, 9)`)
                let callee_ty = self.resolve_fn_callee_ty(st, callee)?;
                if let TypeExpr::Fn { results, .. } = callee_ty {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            other => {
                let callee_ty = self.resolve_fn_callee_ty(st, other)?;
                if let TypeExpr::Fn { results, .. } = callee_ty {
                    return Some((callee.render(), results.len()));
                }
                None
            }
        }
    }

    fn resolve_fn_callee_ty(&self, st: &FnState, expr: &Expr) -> Option<TypeExpr> {
        if let Some(ty) = self.ty_of_expr(st, expr) {
            let resolved = self.resolve_type_alias(&ty);
            if matches!(resolved, TypeExpr::Fn { .. }) {
                return Some(resolved);
            }
        }
        match unwrap_parens_and_move(expr) {
            Expr::Ident { name, span } => {
                if let Some((params, results, _)) = st.closure_defs.get(name.as_str()) {
                    return Some(TypeExpr::Fn {
                        params: params.clone(),
                        results: results.iter().map(|r| r.ty.clone()).collect(),
                        span: *span,
                    });
                }
                let root = crate::freeze::resolve_alias(st, name);
                if root != *name {
                    if let Some(rb) = self.lookup_binding(st, &root) {
                        if let Some(ty) = &rb.ty {
                            return Some(self.resolve_type_alias(ty));
                        }
                    }
                    if let Some((params, results, _)) = st.closure_defs.get(root.as_str()) {
                        return Some(TypeExpr::Fn {
                            params: params.clone(),
                            results: results.iter().map(|r| r.ty.clone()).collect(),
                            span: *span,
                        });
                    }
                    let rhead = Self::base_named_type_str(&root);
                    if let Some(decl) = self.fns.get(root.as_str()).or_else(|| self.fns.get(rhead))
                    {
                        return Some(TypeExpr::Fn {
                            params: decl.params.clone(),
                            results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                            span: *span,
                        });
                    }
                }
                if let Some(cval) = self.const_values.get(name.as_str()) {
                    return self.resolve_fn_callee_ty(st, cval);
                }
                None
            }
            Expr::Index { base, .. } => {
                if let Expr::SliceLit {
                    elem: None, items, ..
                } = unwrap_parens_and_move(base)
                {
                    if let Some(first) = items.first() {
                        return self.resolve_fn_callee_ty(st, first);
                    }
                }
                let base_ty = match unwrap_parens_and_move(base) {
                    Expr::Ident { name, .. } => {
                        let root = crate::freeze::resolve_alias(st, name);
                        self.lookup_binding(st, &root)
                            .and_then(|b| b.ty.clone())
                            .or_else(|| {
                                self.const_values
                                    .get(root.as_str())
                                    .and_then(|cv| match unwrap_parens_and_move(cv) {
                                        Expr::SliceLit {
                                            elem: None,
                                            items,
                                            span,
                                        } => items
                                            .first()
                                            .and_then(|f| self.resolve_fn_callee_ty(st, f))
                                            .map(|elem| TypeExpr::Slice {
                                                elem: Box::new(elem),
                                                span: *span,
                                            }),
                                        _ => self.ty_of_expr(st, cv),
                                    })
                            })
                    }
                    _ => self.ty_of_expr(st, base),
                }?;
                match self.resolve_type_alias(&base_ty) {
                    TypeExpr::Slice { elem, .. }
                    | TypeExpr::Array { elem, .. }
                    | TypeExpr::Map { value: elem, .. } => Some(self.resolve_type_alias(&elem)),
                    _ => None,
                }
            }
            Expr::Call { callee, .. } => {
                if let Some(TypeExpr::Fn { results, .. }) = self.resolve_fn_callee_ty(st, callee) {
                    return results.first().map(|r| self.resolve_type_alias(r));
                }
                None
            }
            _ => None,
        }
    }

    fn buildable_owns_multi_value_call(&self, st: &FnState, expr: &Expr) -> bool {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return false;
        };
        match unwrap_parens(callee) {
            Expr::FnLit { results, .. } => results.len() >= 2,
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    return b
                        .ty
                        .as_ref()
                        .map(|t| self.resolve_type_alias(t))
                        .is_some_and(|t| matches!(t, TypeExpr::Fn { results, .. } if results.len() >= 2));
                }
                let head = Self::base_named_type_str(name);
                self.fns
                    .get(name.as_str())
                    .or_else(|| self.fns.get(head))
                    .is_some_and(|f| f.results.len() >= 2)
            }
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() {
                        let head = Self::base_named_type_str(method_name);
                        let full = format!("{pkg}.{head}");
                        if self.fns.contains_key(full.as_str())
                            || foreign::package_result_count(pkg.as_str(), head).is_some()
                        {
                            return false;
                        }
                    }
                }
                true
            }
            _ => false,
        }
    }

    /// Check file-level `const` initializers (including nested closures) for
    /// multi-value arity violations (T23).
    pub(crate) fn check_const_multi_value(&mut self, value: &Expr) {
        let st = FnState::default();
        self.check_subexprs_single_value(&st, value);
        self.check_const_expr_fnlits(&st, value);
    }

    fn check_const_expr_fnlits(&mut self, st: &FnState, expr: &Expr) {
        let mut flits = Vec::new();
        crate::walk::expr_fnlits(expr, &mut flits);
        for flit in flits {
            if let Expr::FnLit {
                params,
                results,
                body,
                ..
            } = unwrap_parens(flit)
            {
                self.check_const_closure(st, params, results, body);
            }
        }
    }

    fn check_const_closure(
        &mut self,
        outer_st: &FnState,
        params: &[Param],
        results: &[FnResult],
        body: &Block,
    ) {
        let mut cst = outer_st.clone();
        cst.result_shapes = results
            .iter()
            .map(|r| self.shape_of_type(&r.ty))
            .collect();
        for p in params {
            let shape = self.shape_of_type(&p.ty);
            cst.env.insert(
                p.name.clone(),
                Binding {
                    shape,
                    ty: Some(p.ty.clone()),
                    immutable: false,
                    is_let: false,
                    managed_taint: false,
                    moved: None,
                    non_owned_origin: None,
                    slot_loan: false,
                },
            );
        }
        for r in results {
            if let Some(name) = &r.name {
                if name != "_" {
                    let shape = self.shape_of_type(&r.ty);
                    cst.env.insert(
                        name.clone(),
                        Binding {
                            shape,
                            ty: Some(r.ty.clone()),
                            immutable: false,
                            is_let: false,
                            managed_taint: false,
                            moved: None,
                            non_owned_origin: None,
                            slot_loan: false,
                        },
                    );
                }
            }
        }
        cst.scope_push();
        self.check_const_closure_block(&mut cst, body);
        cst.scope_pop();
    }

    fn check_const_closure_block(&mut self, st: &mut FnState, block: &Block) {
        for stmt in &block.stmts {
            self.check_const_closure_stmt(st, stmt);
        }
    }

    fn check_const_closure_stmt(&mut self, st: &mut FnState, stmt: &Stmt) {
        self.check_stmt_multi_value(st, stmt);
        let mut flits = Vec::new();
        crate::walk::own_fnlits(stmt, &mut flits);
        for flit in flits {
            if let Expr::FnLit {
                params,
                results,
                body,
                ..
            } = unwrap_parens(flit)
            {
                self.check_const_closure(st, params, results, body);
            }
        }
        match stmt {
            Stmt::Local(l) => {
                let immutable = l.kind == LocalKind::Const;
                let is_let = l.kind == LocalKind::Let;
                if l.values.is_empty() {
                    let shape = l
                        .ty
                        .as_ref()
                        .map(|t| self.shape_of_type(t))
                        .unwrap_or(TypeShape::Unknown);
                    for name in &l.names {
                        self.define_binding(
                            st,
                            name,
                            shape.clone(),
                            l.ty.clone(),
                            immutable,
                            false,
                            is_let,
                        );
                    }
                } else if l.names.len() == l.values.len() {
                    let pre_st = st.clone();
                    for (name, val) in l.names.iter().zip(l.values.iter()) {
                        let shape = self.shape_of_expr(&pre_st, val);
                        let ty = self.ty_of_expr(&pre_st, val).or_else(|| l.ty.clone());
                        self.define_binding(st, name, shape, ty, immutable, false, is_let);
                        match unwrap_parens_and_move(val) {
                            Expr::Ident { name: src, .. } if src != name => {
                                st.aliases.insert(name.clone(), src.clone());
                            }
                            Expr::FnLit {
                                params,
                                results,
                                body,
                                ..
                            } => {
                                st.closure_defs.insert(
                                    name.clone(),
                                    (params.clone(), results.clone(), body.clone()),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                ..
            } if targets.len() == values.len() => {
                let pre_st = st.clone();
                for (t, val) in targets.iter().zip(values.iter()) {
                    if let PlaceKind::Ident(name) = &t.kind {
                        let ty = self.ty_of_expr(&pre_st, val);
                        if let Some(b) = st.env_lookup_mut(name) {
                            if ty.is_some() || b.ty.is_none() {
                                b.ty = ty;
                            }
                        }
                        st.aliases.remove(name);
                        st.closure_defs.remove(name);
                        match unwrap_parens_and_move(val) {
                            Expr::Ident { name: src, .. } if src != name => {
                                st.aliases.insert(name.clone(), src.clone());
                            }
                            Expr::FnLit {
                                params,
                                results,
                                body,
                                ..
                            } => {
                                st.closure_defs.insert(
                                    name.clone(),
                                    (params.clone(), results.clone(), body.clone()),
                                );
                            }
                            _ => {}
                        }
                    }
                }
            }
            Stmt::If {
                init,
                then,
                otherwise,
                ..
            } => {
                st.scope_push();
                if let Some(i) = init {
                    self.check_const_closure_stmt(st, i);
                }
                self.check_const_closure_block(st, then);
                if let Some(o) = otherwise {
                    self.check_const_closure_stmt(st, o);
                }
                st.scope_pop();
            }
            Stmt::For { body, .. } | Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                st.scope_push();
                self.check_const_closure_block(st, body);
                st.scope_pop();
            }
            Stmt::Match { arms, .. } => {
                for arm in arms {
                    st.scope_push();
                    self.check_const_closure_block(st, &arm.body);
                    st.scope_pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    st.scope_push();
                    self.check_const_closure_block(st, &arm.body);
                    st.scope_pop();
                }
            }
            _ => {}
        }
    }
}

