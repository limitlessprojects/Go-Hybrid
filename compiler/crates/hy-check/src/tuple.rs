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

use crate::buildable::OpKind;
use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
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
            Expr::Index { base, .. } => {
                let mut shape = self.shape_of_expr(st, base);
                while let TypeShape::Pointer(inner) = shape {
                    shape = *inner;
                }
                match shape {
                    TypeShape::Map(..) => Some(BindSourceBound::AtMost(2)),
                    TypeShape::Slice(..) | TypeShape::Array(..) | TypeShape::Str => {
                        Some(BindSourceBound::Exact(1))
                    }
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn is_single_value_builtin_or_conversion(&self, name: &str) -> bool {
        let base = Self::base_named_type_str(name);
        matches!(
            base,
            "len"
                | "cap"
                | "make"
                | "new"
                | "append"
                | "copy"
                | "min"
                | "max"
                | "real"
                | "imag"
                | "complex"
                | "recover"
                | "some"
                | "ok"
                | "err"
        ) || crate::const_init::is_conversion_type_name(base)
            || self.type_aliases.contains_key(name)
            || self.type_aliases.contains_key(base)
    }

    fn resolve_receiver_struct_name(&self, st: &FnState, base: &Expr) -> Option<String> {
        if let Some(mut ty) = self.ty_of_expr(st, base) {
            loop {
                ty = self.resolve_type_alias(&ty);
                if let TypeExpr::Pointer { to, .. } = ty {
                    ty = *to;
                } else {
                    break;
                }
            }
            if let Some(name) = Self::base_named_type(&ty) {
                let resolved = self.resolve_type_alias(&TypeExpr::Named {
                    name: name.clone(),
                    span: base.span(),
                });
                if let Some(target) = Self::base_named_type(&resolved) {
                    return Some(target);
                }
                return Some(name);
            }
        }
        let mut shape = self.shape_of_expr(st, base);
        while let TypeShape::Pointer(inner) = shape {
            shape = *inner;
        }
        match shape {
            TypeShape::Struct(Some(s)) => Some(Self::base_named_type_str(&s).to_string()),
            _ => None,
        }
    }

    /// `Some((callee, result count))` when `expr` is a direct or indirect call
    /// (or `?`-unwrapped call) with a statically known result count.
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        if let Expr::Try { operand, .. } = unwrap_parens_and_move(expr) {
            if let Expr::Call { callee, .. } = unwrap_parens_and_move(operand) {
                if let Expr::Field {
                    base,
                    name: method_name,
                    ..
                } = unwrap_parens_and_move(callee)
                {
                    if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                        if self.lookup_binding(st, pkg).is_none()
                            && pkg == "httptest"
                            && (method_name == "get" || method_name == "post")
                        {
                            return Some((format!("httptest.{method_name}?"), 2));
                        }
                    }
                }
            }
            let (name, total) = self.static_result_count(st, operand)?;
            if total >= 2 {
                return Some((format!("{name}?"), total - 1));
            }
            return None;
        }
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
                    return None;
                }
                // 2. Direct call to a file-local top-level function
                let head = Self::base_named_type_str(name);
                if let Some(decl) = self.fns.get(name.as_str()).or_else(|| self.fns.get(head)) {
                    return Some((name.clone(), decl.results.len()));
                }
                // 3. Unshadowed 1-value builtins, prelude constructors, and type conversions
                if !self.structs.contains_key(name.as_str())
                    && !self.structs.contains_key(head)
                    && self.is_single_value_builtin_or_conversion(name)
                {
                    return Some((name.clone(), 1));
                }
                None
            }
            Expr::SliceLit {
                elem: Some(_),
                items,
                ..
            } if items.is_empty() => Some((callee.render(), 1)),
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
                // 2. User-defined struct method (`b.pair()`, including pointer to type-aliased struct)
                if let Some(rb) = self.resolve_receiver_struct_name(st, base) {
                    if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                        return Some((format!("{}.{method_name}", base.render()), decl.results.len()));
                    }
                }
                // 3. Struct field holding a fn value (`h.mm(3, 9)`)
                let callee_ty = self.ty_of_expr(st, callee)?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            other => {
                let callee_ty = self.ty_of_expr(st, other)?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
        }
    }
}

