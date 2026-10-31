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
//! differs from the result count. Covers direct top-level fn calls, explicit
//! generic fn/method calls (`pair[int](3, 9)`, `b.pair[int](3, 9)`), fn-value
//! bindings / aliases (including type-aliased `fn` types, method values
//! `f := b.pair`, generic fn values `f := pair[int]`, channel receives
//! `f := <-ch`, and `for`/`select` bindings), declared and inline closures,
//! user-defined struct methods, indirect `fn`-typed callees (generic and
//! non-generic struct fields, array/slice/map elements, channel receives,
//! higher-order calls), and curated foreign package calls. `?`-unwrapped calls
//! stay permissive (unwrapping the error produces a single value).

use std::collections::HashMap;

use crate::buildable::OpKind;
use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Block, Decl, Expr, ForKind, InterpPart, Param, Place, PlaceKind, SelectArmKind,
    Stmt, TypeExpr,
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
                    if let Some(ty) = self.resolved_callee_ty(st, callee, 0) {
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
                if let Some(rb) = self.resolve_receiver_struct_name(st, base) {
                    if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                        return Some((
                            format!("{}.{method_name}", base.render()),
                            decl.results.len(),
                        ));
                    }
                }
                // 3. Struct field holding a fn value (`h.mm(3, 9)`, including generic `holder[fn(...) (...)]`)
                let callee_ty = self
                    .resolved_callee_ty(st, callee, 0)
                    .or_else(|| self.ty_of_expr(st, callee))?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            Expr::Index { base, .. } => {
                // 1. Single-type-arg generic function call (`pair[int](3, 9)`)
                if let Expr::Ident { name, .. } = unwrap_parens_and_move(base) {
                    if self.lookup_binding(st, name).is_none() {
                        let head = Self::base_named_type_str(name);
                        if let Some(decl) =
                            self.fns.get(name.as_str()).or_else(|| self.fns.get(head))
                        {
                            return Some((callee.render(), decl.results.len()));
                        }
                    }
                }
                // 2. Single-type-arg generic method or package call (`b.pair[int](3, 9)`)
                if let Expr::Field {
                    base: recv,
                    name: method_name,
                    ..
                } = unwrap_parens_and_move(base)
                {
                    let head = Self::base_named_type_str(method_name);
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(recv) {
                        if self.lookup_binding(st, pkg).is_none() {
                            let full = format!("{pkg}.{head}");
                            if let Some(decl) = self.fns.get(full.as_str()) {
                                return Some((callee.render(), decl.results.len()));
                            }
                        }
                    }
                    if let Some(rb) = self.resolve_receiver_struct_name(st, recv) {
                        if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                            return Some((callee.render(), decl.results.len()));
                        }
                    }
                }
                // 3. Slice/array/map element holding a fn value (`fns[0](3, 9)`)
                let callee_ty = self
                    .resolved_callee_ty(st, callee, 0)
                    .or_else(|| self.ty_of_expr(st, callee))?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            other => {
                let callee_ty = self
                    .resolved_callee_ty(st, other, 0)
                    .or_else(|| self.ty_of_expr(st, other))?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
        }
    }

    fn resolve_receiver_struct_name(&self, st: &FnState, base: &Expr) -> Option<String> {
        self.resolved_callee_ty(st, base, 0)
            .or_else(|| self.ty_of_expr(st, base))
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

    fn resolved_callee_ty(&self, st: &FnState, expr: &Expr, depth: usize) -> Option<TypeExpr> {
        if depth > 8 {
            return None;
        }
        match unwrap_parens_and_move(expr) {
            Expr::Ident { name, span } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        return Some(ty.clone());
                    }
                    let root = crate::freeze::resolve_alias(st, name);
                    if root != *name {
                        if let Some(rb) = self.lookup_binding(st, &root) {
                            if let Some(ty) = &rb.ty {
                                return Some(ty.clone());
                            }
                        }
                    }
                    if let Some(inferred) =
                        self.infer_untyped_binding_fn_ty(st, name, *span, depth + 1)
                    {
                        return Some(inferred);
                    }
                    return None;
                }
                let head = Self::base_named_type_str(name);
                if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                    let mut fn_ty = TypeExpr::Fn {
                        params: decl.params.clone(),
                        results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                        span: *span,
                    };
                    if !decl.type_params.is_empty() {
                        if let Some(type_args) = parse_bracket_type_args(name) {
                            let mut subst = HashMap::new();
                            for (tp, arg) in decl.type_params.iter().zip(type_args.into_iter()) {
                                subst.insert(tp.name.clone(), arg);
                            }
                            fn_ty = subst_type_expr(&fn_ty, &subst);
                        }
                    }
                    return Some(fn_ty);
                }
                None
            }
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
            Expr::Field { base, name, span } => {
                if let Expr::StructLit { fields, .. } = unwrap_parens_and_move(base) {
                    if let Some(f) = fields.iter().find(|f| &f.name == name) {
                        if let Some(fty) = self.resolved_callee_ty(st, &f.value, depth + 1) {
                            return Some(fty);
                        }
                    }
                }
                if let Some(base_ty) = self.resolved_callee_ty(st, base, depth + 1) {
                    if let Some(fty) = self.resolve_struct_field_ty_substituted(&base_ty, name) {
                        return Some(fty);
                    }
                }
                if let Some(rb) = self.resolve_receiver_struct_name(st, base) {
                    let head = Self::base_named_type_str(name);
                    if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                        let mut fn_ty = TypeExpr::Fn {
                            params: decl.params.clone(),
                            results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                            span: *span,
                        };
                        if !decl.type_params.is_empty() {
                            if let Some(type_args) = parse_bracket_type_args(name) {
                                let mut subst = HashMap::new();
                                for (tp, arg) in decl.type_params.iter().zip(type_args.into_iter())
                                {
                                    subst.insert(tp.name.clone(), arg);
                                }
                                fn_ty = subst_type_expr(&fn_ty, &subst);
                            }
                        }
                        return Some(fn_ty);
                    }
                }
                self.ty_of_expr(st, expr)
            }
            Expr::Index { base, index, span } => {
                // Single-type-arg generic fn value (`pair[int]`)
                if let Expr::Ident { name, .. } = unwrap_parens_and_move(base) {
                    if self.lookup_binding(st, name).is_none() {
                        let head = Self::base_named_type_str(name);
                        if let Some(decl) =
                            self.fns.get(name.as_str()).or_else(|| self.fns.get(head))
                        {
                            let mut fn_ty = TypeExpr::Fn {
                                params: decl.params.clone(),
                                results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                                span: *span,
                            };
                            if let (Some(tp), Some(arg_ty)) = (
                                decl.type_params.first(),
                                parse_single_type_expr(&index.render()),
                            ) {
                                let mut subst = HashMap::new();
                                subst.insert(tp.name.clone(), arg_ty);
                                fn_ty = subst_type_expr(&fn_ty, &subst);
                            }
                            return Some(fn_ty);
                        }
                    }
                }
                // Single-type-arg generic method value (`b.pair[int]`)
                if let Expr::Field {
                    base: recv,
                    name: method_name,
                    ..
                } = unwrap_parens_and_move(base)
                {
                    if let Some(rb) = self.resolve_receiver_struct_name(st, recv) {
                        let head = Self::base_named_type_str(method_name);
                        if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                            let mut fn_ty = TypeExpr::Fn {
                                params: decl.params.clone(),
                                results: decl.results.iter().map(|r| r.ty.clone()).collect(),
                                span: *span,
                            };
                            if let (Some(tp), Some(arg_ty)) = (
                                decl.type_params.first(),
                                parse_single_type_expr(&index.render()),
                            ) {
                                let mut subst = HashMap::new();
                                subst.insert(tp.name.clone(), arg_ty);
                                fn_ty = subst_type_expr(&fn_ty, &subst);
                            }
                            return Some(fn_ty);
                        }
                    }
                }
                if let Some(base_ty) = self.resolved_callee_ty(st, base, depth + 1) {
                    let mut cur = self.resolve_type_alias(&base_ty);
                    while let TypeExpr::Pointer { to, .. } = cur {
                        cur = self.resolve_type_alias(&to);
                    }
                    match cur {
                        TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                            return Some(*elem)
                        }
                        TypeExpr::Map { value, .. } => return Some(*value),
                        _ => {}
                    }
                }
                self.ty_of_expr(st, expr)
            }
            Expr::Receive { chan, .. } => {
                let chan_ty = self.resolved_callee_ty(st, chan, depth + 1)?;
                match self.resolve_type_alias(&chan_ty) {
                    TypeExpr::Chan { elem, .. } => Some(*elem),
                    _ => None,
                }
            }
            Expr::Call { callee, .. } => {
                if let Some(callee_ty) = self.resolved_callee_ty(st, callee, depth + 1) {
                    if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                        return results.first().cloned();
                    }
                }
                self.ty_of_expr(st, expr)
            }
            _ => self.ty_of_expr(st, expr),
        }
    }

    fn resolve_struct_field_ty_substituted(
        &self,
        base_ty: &TypeExpr,
        field_name: &str,
    ) -> Option<TypeExpr> {
        let mut cur = self.resolve_type_alias(base_ty);
        while let TypeExpr::Pointer { to, .. } = cur {
            cur = self.resolve_type_alias(&to);
        }
        let TypeExpr::Named { name, .. } = &cur else {
            return None;
        };
        let sname = Self::base_named_type_str(name);
        let fields = self.structs.get(sname)?;
        let (_, raw_field_ty) = fields.iter().find(|(n, _)| n == field_name)?;
        if let Some(tparams) = self.struct_type_params.get(sname) {
            if !tparams.is_empty() {
                if let Some(type_args) = parse_bracket_type_args(name) {
                    let mut subst = HashMap::new();
                    for (tp, arg) in tparams.iter().zip(type_args.into_iter()) {
                        subst.insert(tp.name.clone(), arg);
                    }
                    if !subst.is_empty() {
                        return Some(subst_type_expr(raw_field_ty, &subst));
                    }
                }
            }
        }
        Some(raw_field_ty.clone())
    }

    fn infer_untyped_binding_fn_ty(
        &self,
        st: &FnState,
        target_name: &str,
        use_span: Span,
        depth: usize,
    ) -> Option<TypeExpr> {
        let mut found: Option<TypeExpr> = None;
        for d in &self.file.decls {
            if let Decl::Fn(f) = d {
                if span_contains(f.span, use_span) {
                    if let Some(body) = &f.body {
                        self.find_binding_ty_in_block(
                            st,
                            body,
                            target_name,
                            use_span,
                            depth,
                            &mut found,
                        );
                    }
                }
            }
        }
        found
    }

    fn find_binding_ty_in_block(
        &self,
        st: &FnState,
        block: &Block,
        target_name: &str,
        use_span: Span,
        depth: usize,
        out: &mut Option<TypeExpr>,
    ) {
        for s in &block.stmts {
            if s.span().start.line > use_span.start.line
                || (s.span().start.line == use_span.start.line
                    && s.span().start.col > use_span.start.col)
            {
                break;
            }
            self.find_binding_ty_in_stmt(st, s, target_name, use_span, depth, out);
        }
    }

    fn find_binding_ty_in_stmt(
        &self,
        st: &FnState,
        stmt: &Stmt,
        target_name: &str,
        use_span: Span,
        depth: usize,
        out: &mut Option<TypeExpr>,
    ) {
        match stmt {
            Stmt::Local(l) => {
                if l.names.len() == l.values.len() {
                    for (n, v) in l.names.iter().zip(l.values.iter()) {
                        if n == target_name {
                            *out = self.resolved_callee_ty(st, v, depth + 1);
                        }
                        self.find_binding_ty_in_expr(st, v, target_name, use_span, depth, out);
                    }
                }
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                ..
            } => {
                if targets.len() == values.len() {
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if let PlaceKind::Ident(n) = &t.kind {
                            if n == target_name {
                                *out = self.resolved_callee_ty(st, v, depth + 1);
                            }
                        }
                        self.find_binding_ty_in_expr(st, v, target_name, use_span, depth, out);
                    }
                }
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                span,
            } => {
                if span_contains(*span, use_span) {
                    if let Some(i) = init {
                        self.find_binding_ty_in_stmt(st, i, target_name, use_span, depth, out);
                    }
                    self.find_binding_ty_in_expr(st, cond, target_name, use_span, depth, out);
                    if span_contains(then.span, use_span) {
                        self.find_binding_ty_in_block(st, then, target_name, use_span, depth, out);
                    }
                    if let Some(o) = otherwise {
                        if span_contains(o.span(), use_span) {
                            self.find_binding_ty_in_stmt(st, o, target_name, use_span, depth, out);
                        }
                    }
                }
            }
            Stmt::For { kind, body, span } => {
                if span_contains(*span, use_span) {
                    if let ForKind::Iterate { vars, expr } = kind {
                        if let Some(idx) = vars.iter().position(|v| v == target_name) {
                            if let Some(iter_ty) = self.resolved_callee_ty(st, expr, depth + 1) {
                                let resolved = self.resolve_type_alias(&iter_ty);
                                *out = match resolved {
                                    TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => {
                                        if vars.len() == 1 || idx == 1 {
                                            Some(*elem)
                                        } else {
                                            None
                                        }
                                    }
                                    TypeExpr::Map { key, value, .. } => {
                                        if vars.len() >= 2 {
                                            if idx == 0 {
                                                Some(*key)
                                            } else {
                                                Some(*value)
                                            }
                                        } else {
                                            Some(*value)
                                        }
                                    }
                                    TypeExpr::Chan { elem, .. } => Some(*elem),
                                    _ => None,
                                };
                            }
                        }
                    }
                    self.find_binding_ty_in_block(st, body, target_name, use_span, depth, out);
                }
            }
            Stmt::Select { arms, span } => {
                if span_contains(*span, use_span) {
                    for arm in arms {
                        if span_contains(arm.body.span, use_span) {
                            if let SelectArmKind::Recv { names, chan } = &arm.kind {
                                if names.first().is_some_and(|n| n == target_name) {
                                    if let Some(chan_ty) =
                                        self.resolved_callee_ty(st, chan, depth + 1)
                                    {
                                        if let TypeExpr::Chan { elem, .. } =
                                            self.resolve_type_alias(&chan_ty)
                                        {
                                            *out = Some(*elem);
                                        }
                                    }
                                }
                            }
                            self.find_binding_ty_in_block(
                                st,
                                &arm.body,
                                target_name,
                                use_span,
                                depth,
                                out,
                            );
                        }
                    }
                }
            }
            Stmt::Match { arms, span, .. } => {
                if span_contains(*span, use_span) {
                    for arm in arms {
                        if span_contains(arm.body.span, use_span) {
                            self.find_binding_ty_in_block(
                                st,
                                &arm.body,
                                target_name,
                                use_span,
                                depth,
                                out,
                            );
                        }
                    }
                }
            }
            Stmt::Managed { body, span, .. }
            | Stmt::Block {
                block: body, span, ..
            } => {
                if span_contains(*span, use_span) {
                    self.find_binding_ty_in_block(st, body, target_name, use_span, depth, out);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.find_binding_ty_in_expr(st, expr, target_name, use_span, depth, out);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.find_binding_ty_in_expr(st, v, target_name, use_span, depth, out);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.find_binding_ty_in_expr(st, callee, target_name, use_span, depth, out);
                for a in args {
                    self.find_binding_ty_in_expr(st, &a.value, target_name, use_span, depth, out);
                }
            }
            _ => {}
        }
    }

    fn find_binding_ty_in_expr(
        &self,
        st: &FnState,
        expr: &Expr,
        target_name: &str,
        use_span: Span,
        depth: usize,
        out: &mut Option<TypeExpr>,
    ) {
        if !span_contains(expr.span(), use_span) {
            return;
        }
        match expr {
            Expr::FnLit { body, .. } => {
                self.find_binding_ty_in_block(st, body, target_name, use_span, depth, out);
            }
            Expr::Call { callee, args, .. } => {
                self.find_binding_ty_in_expr(st, callee, target_name, use_span, depth, out);
                for a in args {
                    self.find_binding_ty_in_expr(st, &a.value, target_name, use_span, depth, out);
                }
            }
            Expr::Paren { inner, .. }
            | Expr::Move { operand: inner, .. }
            | Expr::Try { operand: inner, .. }
            | Expr::Unary { operand: inner, .. }
            | Expr::AddrOf { operand: inner, .. }
            | Expr::Field { base: inner, .. }
            | Expr::Receive { chan: inner, .. } => {
                self.find_binding_ty_in_expr(st, inner, target_name, use_span, depth, out);
            }
            Expr::Binary { lhs, rhs, .. }
            | Expr::Index {
                base: lhs,
                index: rhs,
                ..
            } => {
                self.find_binding_ty_in_expr(st, lhs, target_name, use_span, depth, out);
                self.find_binding_ty_in_expr(st, rhs, target_name, use_span, depth, out);
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.find_binding_ty_in_expr(st, &f.value, target_name, use_span, depth, out);
                }
            }
            Expr::SliceLit { items, .. } => {
                for item in items {
                    self.find_binding_ty_in_expr(st, item, target_name, use_span, depth, out);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.find_binding_ty_in_expr(st, k, target_name, use_span, depth, out);
                    self.find_binding_ty_in_expr(st, v, target_name, use_span, depth, out);
                }
            }
            _ => {}
        }
    }
}

fn span_contains(outer: Span, inner: Span) -> bool {
    let after_start = inner.start.line > outer.start.line
        || (inner.start.line == outer.start.line && inner.start.col >= outer.start.col);
    let before_end = inner.end.line < outer.end.line
        || (inner.end.line == outer.end.line && inner.end.col <= outer.end.col);
    after_start && before_end
}

fn parse_single_type_expr(src: &str) -> Option<TypeExpr> {
    let trimmed = src.trim();
    if trimmed.is_empty() {
        return None;
    }
    let snippet = format!("type _t = {}\n", trimmed);
    let file = hy_syntax::parse_file("", &snippet).ok()?;
    match file.decls.into_iter().next()? {
        Decl::TypeAlias(ta) => Some(ta.ty),
        _ => None,
    }
}

fn parse_bracket_type_args(name: &str) -> Option<Vec<TypeExpr>> {
    let open = name.find('[')?;
    if !name.ends_with(']') || open + 1 >= name.len() - 1 {
        return None;
    }
    let inner = &name[open + 1..name.len() - 1];
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (idx, ch) in inner.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(&inner[start..idx]);
                start = idx + 1;
            }
            _ => {}
        }
    }
    parts.push(&inner[start..]);
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        out.push(parse_single_type_expr(part)?);
    }
    Some(out)
}

fn subst_type_expr(ty: &TypeExpr, subst: &HashMap<String, TypeExpr>) -> TypeExpr {
    if subst.is_empty() {
        return ty.clone();
    }
    match ty {
        TypeExpr::Named { name, span } => {
            if let Some(repl) = subst.get(name) {
                return repl.clone();
            }
            if let Some(open) = name.find('[') {
                if let Some(args) = parse_bracket_type_args(name) {
                    let base = &name[..open];
                    let joined = args
                        .iter()
                        .map(|a| subst_type_expr(a, subst).render())
                        .collect::<Vec<_>>()
                        .join(", ");
                    return TypeExpr::Named {
                        name: format!("{}[{}]", base, joined),
                        span: *span,
                    };
                }
            }
            ty.clone()
        }
        TypeExpr::Pointer { to, span } => TypeExpr::Pointer {
            to: Box::new(subst_type_expr(to, subst)),
            span: *span,
        },
        TypeExpr::Slice { elem, span } => TypeExpr::Slice {
            elem: Box::new(subst_type_expr(elem, subst)),
            span: *span,
        },
        TypeExpr::Array { len, elem, span } => TypeExpr::Array {
            len: len.clone(),
            elem: Box::new(subst_type_expr(elem, subst)),
            span: *span,
        },
        TypeExpr::Map { key, value, span } => TypeExpr::Map {
            key: Box::new(subst_type_expr(key, subst)),
            value: Box::new(subst_type_expr(value, subst)),
            span: *span,
        },
        TypeExpr::Chan { dir, elem, span } => TypeExpr::Chan {
            dir: *dir,
            elem: Box::new(subst_type_expr(elem, subst)),
            span: *span,
        },
        TypeExpr::Option { elem, span } => TypeExpr::Option {
            elem: Box::new(subst_type_expr(elem, subst)),
            span: *span,
        },
        TypeExpr::Fn {
            params,
            results,
            span,
        } => TypeExpr::Fn {
            params: params
                .iter()
                .map(|p| Param {
                    name: p.name.clone(),
                    ty: subst_type_expr(&p.ty, subst),
                    span: p.span,
                })
                .collect(),
            results: results.iter().map(|r| subst_type_expr(r, subst)).collect(),
            span: *span,
        },
    }
}
