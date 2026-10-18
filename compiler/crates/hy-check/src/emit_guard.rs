//! emit_guard.rs — expression/type checks that keep accepted programs
//! emitting buildable Go (issue #201).
//!
//! Families covered here:
//!   * A — `==` / `!=` on an incomparable type (map, slice, function
//!     value, or a struct carrying one): Go rejects it with
//!     `invalid operation: … can only be compared to nil` /
//!     `cannot be compared` (HYB-E2006).
//!   * B4 — a constant negative shift count (`1 << -1`, `x <<= -1`):
//!     Go rejects it with `negative shift count` (HYB-E6005).
//!   * E — array length validity at local ascriptions (`[-1]int`,
//!     `[n]int` with a non-constant `n`): Go rejects it with
//!     `invalid array length` (HYB-E6004). Decl-level array lengths
//!     (params, struct fields, consts) are checked from `build_tables`.
//!
//! The statement/expression walk mirrors `arith.rs` (issue #149's pass):
//! flatten() lists nested bodies as their own entries, so the top-level
//! walk stays shallow (`deep = false`) and closure bodies are recursed
//! exactly once via `Expr::FnLit`.

use crate::const_overflow::{const_float_value, const_int_value};
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, CallArg, Expr, FnDecl, ForKind, InterpPart, Place, PlaceKind, SelectArmKind,
    Stmt, TypeExpr,
};

impl<'a> Checker<'a> {
    /// Pass-B entry: mirror `arith::check_stmt_const_zero_divisor`.
    pub(crate) fn check_stmt_emit_guard(&mut self, st: &FnState, stmt: &Stmt) {
        self.check_stmt_addr_of_loan_projections(st, stmt, /*include_direct_ident=*/ false);
        let mut top_st = st.clone();
        self.check_stmt_emit_guard_inner(&mut top_st, stmt, /*deep=*/ false);
    }

    fn check_stmt_emit_guard_inner(&mut self, st: &mut FnState, stmt: &Stmt, deep: bool) {
        if deep && !matches!(stmt, Stmt::If { .. } | Stmt::Match { .. }) {
            self.check_stmt_addr_of_loan_projections(st, stmt, /*include_direct_ident=*/ true);
        }
        match stmt {
            Stmt::Local(l) => {
                if let Some(t) = &l.ty {
                    self.check_type_expr_with_state(Some(st), t);
                }
                for v in &l.values {
                    self.check_expr_emit_guard(st, v);
                }
                if deep {
                    if matches!(l.kind, hy_syntax::LocalKind::Const) {
                        for (name, value) in l.names.iter().zip(l.values.iter()) {
                            self.check_const_init_is_constant(name, value, Some(st));
                        }
                    }
                    if let Some(ty) = &l.ty {
                        for (name, value) in l.names.iter().zip(l.values.iter()) {
                            self.check_const_overflow_init(name, ty, value, Some(st));
                        }
                    }
                    self.bind_closure_local_for_emit_guard(st, l);
                    self.mark_slot_loans_local(st, l);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.check_place_emit_guard(st, t);
                }
                for v in values {
                    self.check_expr_emit_guard(st, v);
                }
                // `x <<= -1` — the compound form of HYB-E6005.
                if matches!(op, AssignOp::ShlAssign | AssignOp::ShrAssign) && values.len() == 1 {
                    self.check_negative_shift(&values[0], *span);
                }
                if deep {
                    self.mark_slot_loans_assign(st, targets, *op, values);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_emit_guard(st, expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_emit_guard(st, chan);
                self.check_expr_emit_guard(st, value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if deep {
                    st.scope_push();
                    if let Some(init) = init {
                        self.check_stmt_emit_guard_inner(st, init, true);
                    }
                    self.check_expr_addr_of_loan_projections(
                        st,
                        cond,
                        /*include_direct_ident=*/ true,
                    );
                }
                self.check_expr_emit_guard(st, cond);
                if deep {
                    st.scope_push();
                    for s in &then.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                    st.scope_pop();
                    if let Some(other) = otherwise {
                        st.scope_push();
                        self.check_stmt_emit_guard_inner(st, other, true);
                        st.scope_pop();
                    }
                    st.scope_pop();
                }
            }
            Stmt::For { kind, body, .. } => {
                if deep {
                    st.scope_push();
                }
                match kind {
                    ForKind::While { cond } => self.check_expr_emit_guard(st, cond),
                    ForKind::Range { var, lo, hi, .. } => {
                        self.check_expr_emit_guard(st, lo);
                        self.check_expr_emit_guard(st, hi);
                        if deep {
                            self.define_binding(
                                st,
                                var,
                                TypeShape::Primitive,
                                None,
                                false,
                                false,
                            );
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.check_expr_emit_guard(st, expr);
                        if deep {
                            let shapes = self.iterate_var_shapes(st, expr, vars.len());
                            for (name, shape) in vars.iter().zip(shapes) {
                                self.define_binding(st, name, shape, None, false, false);
                            }
                        }
                    }
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                    st.scope_pop();
                }
            }
            Stmt::Return { values, .. } => {
                for (i, v) in values.iter().enumerate() {
                    if let Some(res_ty) = st.result_types.get(i) {
                        let resolved_ty = self.resolve_type_alias(res_ty);
                        self.check_const_overflow_init("return statement", &resolved_ty, v, None);
                    }
                    self.check_expr_emit_guard(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_call_const_overflow(st, callee, args);
                self.check_expr_emit_guard(st, callee);
                for a in args {
                    self.check_expr_emit_guard(st, &a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    st.scope_push();
                    for s in &body.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                    st.scope_pop();
                }
            }
            Stmt::Match { subject, arms, .. } => {
                if deep {
                    self.check_expr_addr_of_loan_projections(
                        st,
                        subject,
                        /*include_direct_ident=*/ true,
                    );
                }
                self.check_expr_emit_guard(st, subject);
                for arm in arms {
                    st.scope_push();
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        self.bind_match_arm(st, b, subject, false);
                    }
                    if let Some(g) = &arm.guard {
                        if deep {
                            self.check_expr_addr_of_loan_projections(
                                st,
                                g,
                                /*include_direct_ident=*/ true,
                            );
                        }
                        self.check_expr_emit_guard(st, g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_emit_guard_inner(st, s, true);
                        }
                    }
                    st.scope_pop();
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_emit_guard(st, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    st.scope_push();
                    for s in &block.stmts {
                        self.check_stmt_emit_guard_inner(st, s, true);
                    }
                    st.scope_pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    if deep {
                        st.scope_push();
                    }
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_emit_guard(st, chan);
                            self.check_expr_emit_guard(st, value);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.check_expr_emit_guard(st, chan);
                            if deep {
                                let elem = match self.shape_of_expr(st, chan) {
                                    TypeShape::Chan(e) => *e,
                                    _ => TypeShape::Unknown,
                                };
                                for (i, n) in names.iter().enumerate() {
                                    let shape = if i == 0 {
                                        elem.clone()
                                    } else {
                                        TypeShape::Primitive
                                    };
                                    self.define_binding(st, n, shape, None, false, false);
                                }
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_emit_guard_inner(st, s, true);
                        }
                        st.scope_pop();
                    }
                }
            }
        }
    }

    fn bind_closure_local_for_emit_guard(&self, st: &mut FnState, l: &hy_syntax::LocalDecl) {
        let immutable = matches!(l.kind, hy_syntax::LocalKind::Const);
        if l.values.is_empty() {
            let shape = l
                .ty
                .as_ref()
                .map(|t| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown);
            let ty = l.ty.clone();
            for name in &l.names {
                self.define_binding(st, name, shape.clone(), ty.clone(), immutable, false);
            }
        } else if l.names.len() == l.values.len() {
            for (name, val) in l.names.iter().zip(l.values.iter()) {
                let inferred = self.shape_of_expr(st, val);
                let shape = match (&l.ty, inferred) {
                    (Some(t), TypeShape::Unknown) => self.shape_of_type(t),
                    (_, s) => s,
                };
                let ty = l.ty.clone().or_else(|| self.ty_of_expr(st, val));
                self.define_binding(st, name, shape, ty, immutable, false);
            }
        }
    }

    fn check_place_emit_guard(&mut self, st: &FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_emit_guard(st, base);
                self.check_expr_emit_guard(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_emit_guard(st, base)
            }
        }
    }

    pub(crate) fn check_expr_emit_guard(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Binary { op, lhs, rhs, span } => {
                self.check_expr_emit_guard(st, lhs);
                self.check_expr_emit_guard(st, rhs);
                match op {
                    BinOp::Eq | BinOp::Ne => self.check_equality(st, lhs, rhs, *span),
                    BinOp::Shl | BinOp::Shr => self.check_negative_shift(rhs, *span),
                    _ => {}
                }
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => self.check_expr_emit_guard(st, operand),
            Expr::Index { base, index, .. } => {
                self.check_expr_emit_guard(st, base);
                self.check_expr_emit_guard(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_emit_guard(st, base);
                if let Some(lo) = lo {
                    self.check_expr_emit_guard(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_emit_guard(st, hi);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.check_call_const_overflow(st, callee, args);
                self.check_expr_emit_guard(st, callee);
                for a in args {
                    self.check_expr_emit_guard(st, &a.value);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.check_type_expr_with_state(Some(st), t);
                }
                if let Some(TypeExpr::Named { name, .. }) = ty {
                    let mut sname = name.clone();
                    while let Some(TypeExpr::Named { name: target, .. }) =
                        self.type_aliases.get(&sname)
                    {
                        sname = target.clone();
                    }
                    if let Some(struct_fields) = self.structs.get(&sname).cloned() {
                        for f in fields {
                            if let Some((_, f_ty)) =
                                struct_fields.iter().find(|(fname, _)| fname == &f.name)
                            {
                                self.check_const_overflow_init(
                                    &format!("{}.{}", sname, f.name),
                                    f_ty,
                                    &f.value,
                                    None,
                                );
                            }
                        }
                    }
                }
                for f in fields {
                    self.check_expr_emit_guard(st, &f.value);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(elem_ty) = elem {
                    self.check_type_expr_with_state(Some(st), elem_ty);
                    let elem_ty = *elem_ty.clone();
                    for i in items {
                        self.check_const_overflow_init("slice literal element", &elem_ty, i, None);
                    }
                }
                for i in items {
                    self.check_expr_emit_guard(st, i);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.check_type_expr_with_state(Some(st), key);
                self.check_type_expr_with_state(Some(st), value);
                let key_ty = *key.clone();
                let val_ty = *value.clone();
                for (k, v) in entries {
                    self.check_const_overflow_init("map literal key", &key_ty, k, None);
                    self.check_const_overflow_init("map literal value", &val_ty, v, None);
                    self.check_expr_emit_guard(st, k);
                    self.check_expr_emit_guard(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_emit_guard(st, e);
                    }
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                // Closure signature types (`fn(a [-1]int) …`) are declared
                // here, not at a local ascription — check them too.
                for p in params {
                    self.check_type_expr_with_state(Some(st), &p.ty);
                }
                for r in results {
                    self.check_type_expr_with_state(Some(st), &r.ty);
                }
                // Closure bodies are not flattened into Pass B — deep-walk
                // with a closure-scoped environment so closure parameters and
                // locals accurately shadow or extend the enclosing function env.
                let mut cst = st.clone();
                cst.scope_push();
                for p in params {
                    let shape = self.shape_of_type(&p.ty);
                    self.define_binding(&mut cst, &p.name, shape, Some(p.ty.clone()), false, false);
                }
                for r in results {
                    if let Some(name) = &r.name {
                        if name != "_" {
                            let shape = self.shape_of_type(&r.ty);
                            self.define_binding(
                                &mut cst,
                                name,
                                shape,
                                Some(r.ty.clone()),
                                false,
                                false,
                            );
                        }
                    }
                }
                cst.result_types = results.iter().map(|r| r.ty.clone()).collect();
                for stmt in &body.stmts {
                    self.check_stmt_emit_guard_inner(&mut cst, stmt, /*deep=*/ true);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_call_const_overflow(&mut self, st: &FnState, callee: &Expr, args: &[CallArg]) {
        let unwrapped = crate::model::unwrap_parens(callee);

        // 1. Built-in: append(slice, items...)
        if let Expr::Ident { name, .. } = unwrapped {
            if name == "append" {
                if let Some(first_arg) = args.first() {
                    let elem_ty = self
                        .ty_of_expr(st, &first_arg.value)
                        .map(|t| self.resolve_type_alias(&t))
                        .and_then(|t| match t {
                            TypeExpr::Slice { elem, .. } => Some(*elem),
                            _ => None,
                        });
                    if let Some(elem_ty) = elem_ty {
                        for a in &args[1..] {
                            if !a.spread {
                                self.check_const_overflow_init("argument of append", &elem_ty, &a.value, None);
                            }
                        }
                    }
                }
                return;
            }
        }

        // 2. Resolve parameter list
        let param_types: Option<Vec<(String, TypeExpr)>> = match unwrapped {
            Expr::Ident { name, .. } => {
                if self.lookup_binding(st, name).is_none() {
                    self.fns.get(name.as_str()).map(|decl| {
                        decl.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect()
                    })
                } else if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(TypeExpr::Fn { params, .. }) = &b.ty {
                        Some(params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect())
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            Expr::Field { base, name: method_name, .. } => {
                let is_pkg = if let Expr::Ident { name: pkg, .. } = crate::model::unwrap_parens(base) {
                    self.lookup_binding(st, pkg).is_none()
                        && self.fns.contains_key(&format!("{}.{}", pkg, method_name))
                } else {
                    false
                };

                if is_pkg {
                    let Expr::Ident { name: pkg, .. } = crate::model::unwrap_parens(base) else {
                        unreachable!()
                    };
                    let full = format!("{}.{}", pkg, method_name);
                    self.fns.get(full.as_str()).map(|decl| {
                        decl.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect()
                    })
                } else {
                    let recv_struct = match self.shape_of_expr(st, base) {
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
                    };
                    let decl = if let Some(s) = recv_struct {
                        self.method_decls.get(&(s, method_name.clone())).copied()
                    } else {
                        self.method_decls
                            .iter()
                            .find(|((_, m), _)| m == method_name)
                            .map(|(_, d)| *d)
                    };
                    decl.map(|d| d.params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect())
                }
            }
            Expr::FnLit { params, .. } => {
                Some(params.iter().map(|p| (p.name.clone(), p.ty.clone())).collect())
            }
            _ => None,
        };

        if let Some(params) = param_types {
            for (i, a) in args.iter().enumerate() {
                if a.spread {
                    continue;
                }
                let target = if let Some(ref arg_name) = a.name {
                    params.iter().find(|(p_name, _)| p_name == arg_name)
                } else {
                    params.get(i)
                };
                if let Some((p_name, p_ty)) = target {
                    let resolved_ty = self.resolve_type_alias(p_ty);
                    self.check_const_overflow_init(
                        &format!("parameter `{}`", p_name),
                        &resolved_ty,
                        &a.value,
                        None,
                    );
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // A — equality on an incomparable type (HYB-E2006)
    // ------------------------------------------------------------------

    fn check_equality(&mut self, st: &FnState, lhs: &Expr, rhs: &Expr, span: Span) {
        // `x == nil` is the nil-comparison surface (E4001/N1 own its
        // validity) — every nilable type is comparable to nil in Go.
        if Checker::is_nil_literal(lhs) || Checker::is_nil_literal(rhs) {
            return;
        }
        let l = self.equality_shape(st, lhs);
        let r = self.equality_shape(st, rhs);
        let (bad, noun) = if let Some(n) = self.incomparable_noun(&l) {
            (lhs, n)
        } else if let Some(n) = self.incomparable_noun(&r) {
            (rhs, n)
        } else {
            return;
        };
        let msg = format!(
            "`{}` compares values of incomparable type ({}) — Go rejects this at compile time (HYB-E2006)",
            bad.render(),
            noun
        );
        let notes = vec![
            format!(
                "why: a `{}` value has no `==` in Go (`invalid operation: … {}`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                noun,
                match noun {
                    "map" => "map can only be compared to nil",
                    "slice" => "slice can only be compared to nil",
                    "function value" => "func can only be compared to nil",
                    _ => "cannot be compared",
                }
            ),
            match noun {
                "map" => "fix: compare membership or length (`len(a) == len(b)`), or a specific entry (`a[k] == b[k]`), instead of whole-map equality".to_string(),
                "slice" => "fix: compare lengths or elements pairwise — whole-slice equality has no Go meaning".to_string(),
                "function value" => "fix: a function value is only comparable to `nil` — compare behaviour (call both) instead".to_string(),
                _ => "fix: compare the comparable fields individually — the struct carries an incomparable field".to_string(),
            },
        ];
        self.diags
            .push(Diagnostic::error("HYB-E2006", self.path.clone(), span, msg).with_notes(notes));
    }

    /// Shape of an equality operand, treating a bare top-level fn name as
    /// a function VALUE (Go: `helper == helper` compares funcs —
    /// `shape_of_expr` only models fn names in call position).
    fn equality_shape(&self, st: &FnState, e: &Expr) -> TypeShape {
        if let Expr::Ident { name, .. } = unwrap_parens(e) {
            if !name.is_empty()
                && self.lookup_binding(st, name).is_none()
                && self.fns.contains_key(name.as_str())
            {
                return TypeShape::Closure;
            }
        }
        self.shape_of_expr(st, e)
    }

    /// `Some(noun)` when a `==` over this shape is provably illegal in
    /// Go. Unknown shapes stay comparable (fail-open: the checker has no
    /// type system — only PROVEN-incomparable shapes reject).
    fn incomparable_noun(&self, shape: &TypeShape) -> Option<&'static str> {
        let mut seen: Vec<String> = Vec::new();
        self.incomparable_rec(shape, &mut seen)
    }

    fn incomparable_rec(&self, shape: &TypeShape, seen: &mut Vec<String>) -> Option<&'static str> {
        match shape {
            TypeShape::Map(_, _) => Some("map"),
            TypeShape::Slice(_) => Some("slice"),
            TypeShape::Closure => Some("function value"),
            // `[N]T` is comparable exactly when `T` is.
            TypeShape::Array(elem) => self.incomparable_rec(elem, seen),
            TypeShape::Struct(Some(name)) => {
                if seen.iter().any(|n| n == name) {
                    return None; // value cycle — Go rejects the type itself elsewhere
                }
                seen.push(name.clone());
                let fields = self.structs.get(name)?;
                for (_, ft) in fields {
                    let fs = self.shape_of_type(ft);
                    if let Some(n) = self.incomparable_rec(&fs, seen) {
                        return Some(n);
                    }
                }
                None
            }
            _ => None,
        }
    }

    // ------------------------------------------------------------------
    // B4 — constant negative shift counts (HYB-E6005)
    // ------------------------------------------------------------------

    fn check_negative_shift(&mut self, rhs: &Expr, span: Span) {
        let Some(v) = const_int_value(rhs) else {
            return;
        };
        if v >= 0 {
            return;
        }
        let msg = format!(
            "shift by constant `{}` — a negative shift count is a compile error in Go (HYB-E6005)",
            rhs.render()
        );
        self.diags.push(
            Diagnostic::error("HYB-E6005", self.path.clone(), span, msg).with_notes(vec![
                format!(
                    "why: Go rejects `<< {}` with `invalid operation: negative shift count {}`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                    v, v
                ),
                "fix: shift by a non-negative constant (shift the other operand instead: `1 << n` with `n >= 0`)".to_string(),
            ]),
        );
    }

    // ------------------------------------------------------------------
    // C — package-scope redeclarations (HYB-E1003)
    // ------------------------------------------------------------------

    /// Record a package-level name (fn / const / struct / enum / type).
    /// Go keeps ONE package block: a second declaration of the same name,
    /// in any of those kinds, is `X redeclared in this block` — the
    /// emitted module would not build (issue #201-C).
    pub(crate) fn declare_pkg_name(&mut self, name: &str, kind: &'static str, span: Span) {
        if let Some(prev) = self.pkg_names.get(name) {
            let msg = format!(
                "`{name}` redeclared in this block — first declared as a {prev}, now as a {kind} (HYB-E1003)",
                name = name,
                prev = prev,
                kind = kind
            );
            self.diags.push(
                Diagnostic::error("HYB-E1003", self.path.clone(), span, msg).with_notes(vec![
                    format!(
                        "why: Go keeps a single package block for fns, consts, structs, enums, and types — a second `{kind}` named `{name}` is `{name} redeclared in this block`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        kind = kind,
                        name = name
                    ),
                    format!(
                        "fix: rename one of the `{name}` declarations — methods may share a name across receiver types (they live in the receiver's method set, not package scope)",
                        name = name
                    ),
                ]),
            );
            return;
        }
        self.pkg_names.insert(name.to_string(), kind);
    }

    /// Register a method: validate the receiver (issue #201-D) and
    /// detect duplicate methods on the same receiver type (issue #201-C).
    pub(crate) fn declare_method(&mut self, f: &FnDecl) {
        let r = f.receiver.as_ref().expect("method has a receiver");
        // The receiver AS WRITTEN: the parser absorbs one `*` into
        // `is_pointer`, so `**box` shows up as `is_pointer + *box`.
        let recv_full = if r.is_pointer {
            format!("*{}", r.ty.render())
        } else {
            r.ty.render()
        };
        // -- D: receiver validity (Go: `invalid receiver type …`) --
        let recv_problem: Option<&'static str> = match &r.ty {
            // The parser absorbs one `*` into `is_pointer`; a stored
            // `*T` here means `**T`, which no Go program can declare.
            TypeExpr::Pointer { .. } => {
                Some("a pointer receiver may point at most one level (`**T` is not a Go receiver)")
            }
            TypeExpr::Named { name, .. } if self.type_aliases.contains_key(name) => {
                Some("a method receiver must be a defined (named) type, not a type alias")
            }
            TypeExpr::Named { name, .. }
                if !self.structs.contains_key(name) && is_primitive_receiver(name) =>
            {
                Some("a method receiver must be a defined (named) type, not a primitive or interface")
            }
            TypeExpr::Slice { .. }
            | TypeExpr::Array { .. }
            | TypeExpr::Map { .. }
            | TypeExpr::Chan { .. }
            | TypeExpr::Option { .. }
            | TypeExpr::Fn { .. } => {
                Some("a method receiver must be a defined (named) type — an anonymous type cannot have methods")
            }
            _ => None,
        };
        if let Some(problem) = recv_problem {
            let msg = format!(
                "invalid receiver type `{}` for method `{}.{}` (HYB-E2007)",
                recv_full, recv_full, f.name
            );
            self.diags.push(
                Diagnostic::error("HYB-E2007", self.path.clone(), r.ty.span(), msg).with_notes(vec![
                    format!("why: {}", problem),
                    format!(
                        "why: Go rejects this with `invalid receiver type {}`, so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        recv_full
                    ),
                    format!(
                        "fix: declare the method on a struct (or named defined type): `struct {} {{ … }}` + `fn (b {}) {}(...)` — or make it a plain fn `fn {}(...)`",
                        r.ty.render(),
                        r.ty.render(),
                        f.name,
                        f.name
                    ),
                ]),
            );
        }
        // -- C: method-set duplication (`method box.get already declared`) --
        let base = r.ty.render();
        if !self.methods.insert((base.clone(), f.name.clone())) {
            let msg = format!("method `{}.{}` already declared (HYB-E1003)", base, f.name);
            self.diags.push(
                Diagnostic::error("HYB-E1003", self.path.clone(), f.span, msg).with_notes(vec![
                    format!(
                        "why: Go rejects a second `func (b {}) {}` on the same receiver type (`method {}.{} already declared`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        base, f.name, base, f.name
                    ),
                    format!(
                        "fix: rename one of the `{}.{}` methods, or define a second receiver type for the overloaded behaviour",
                        base, f.name
                    ),
                ]),
            );
        }
    }

    // ------------------------------------------------------------------
    // E — array length validity (HYB-E6004)
    // ------------------------------------------------------------------

    /// Walk a type expression checking every array length inside it.
    pub(crate) fn check_type_expr(&mut self, ty: &TypeExpr) {
        self.check_type_expr_with_state(None, ty);
    }

    pub(crate) fn check_type_expr_with_state(&mut self, st: Option<&FnState>, ty: &TypeExpr) {
        match ty {
            TypeExpr::Array { len, elem, .. } => {
                if let Some(len) = len {
                    self.check_array_len(st, len);
                }
                self.check_type_expr_with_state(st, elem);
            }
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Option { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. } => self.check_type_expr_with_state(st, to),
            TypeExpr::Map { key, value, .. } => {
                self.check_type_expr_with_state(st, key);
                self.check_type_expr_with_state(st, value);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.check_type_expr_with_state(st, &p.ty);
                }
                for r in results {
                    self.check_type_expr_with_state(st, r);
                }
            }
            TypeExpr::Named { .. } => {}
        }
    }

    fn check_array_len(&mut self, st: Option<&FnState>, len: &Expr) {
        let span = len.span();
        // 1) A constant integer expression: must be non-negative.
        let direct_or_folded = const_int_value(len).or_else(|| {
            if matches!(unwrap_parens(len), Expr::Ident { .. }) {
                None
            } else {
                self.const_int_value_at(len, span.start, 0)
            }
        });
        if let Some(v) = direct_or_folded {
            if v < 0 {
                let msg = format!(
                    "array length `{}` is negative — Go rejects this (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                    len.render(),
                    v
                );
                self.diags.push(
                    Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                        "why: an array length must be a non-negative constant in Go (issue #201)".to_string(),
                        "fix: use a non-negative constant length, or declare a slice (`[]T`) if the size is dynamic".to_string(),
                    ]),
                );
            }
            return;
        }
        // 2) A non-integer constant (`[2.5]int`) — Go: truncated.
        if let Some(f) = const_float_value(len) {
            if f.fract() != 0.0 {
                let msg = format!(
                    "array length `{}` is not an integer — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                    len.render()
                );
                self.diags.push(
                    Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                        "why: an array length must be an integer constant in Go (issue #201)"
                            .to_string(),
                        "fix: round the length to an integer constant, or declare a slice (`[]T`)"
                            .to_string(),
                    ]),
                );
            }
            return;
        }
        // 2b) A boolean, string, or comparison/logical expression (`[true]int`,
        // `[!true]int`, `[1 == 2]int`, `["hi"]int`) is never an integer length.
        if is_non_int_array_len_expr(len) {
            let msg = format!(
                "array length `{}` is not an integer constant — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                len.render()
            );
            self.diags.push(
                Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                    "why: an array length must be a non-negative integer constant in Go (issue #201)"
                        .to_string(),
                    "fix: use a non-negative integer constant length, or declare a slice (`[]T`)"
                        .to_string(),
                ]),
            );
            return;
        }
        // 3) A bare identifier: check local bindings first (shadowing any
        // file-level const), then file-level `const` declarations.
        if let Expr::Ident { name, .. } = unwrap_parens(len) {
            if name.is_empty() {
                return; // inferred-type sentinel, never a length
            }
            if let Some(st) = st {
                if let Some(b) = st.env_lookup(name) {
                    if !b.immutable {
                        let msg = format!(
                            "array length `{}` is not a constant — Go requires a constant array length (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                            name, name
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                                "why: Go's array length is part of the type and must be a non-negative constant; a variable length would change the type (issue #201)".to_string(),
                                "fix: use a literal or `const` length, or declare a slice (`[]T`) for a dynamic size".to_string(),
                            ]),
                        );
                        return;
                    }
                    if !matches!(b.shape, TypeShape::Primitive | TypeShape::Unknown) {
                        let msg = format!(
                            "array length `{}` is not an integer constant — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                            name
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                                "why: an array length must be a non-negative integer constant in Go (issue #201)".to_string(),
                                "fix: declare the `const` with a non-negative integer value, or declare a slice (`[]T`)".to_string(),
                            ]),
                        );
                        return;
                    }
                    let folded = self
                        .resolve_local_const_decl_at(name, span.start)
                        .and_then(|(decl_ty, init_expr, decl_pos)| {
                            if decl_ty.as_ref().is_some_and(is_non_int_primitive_type) {
                                return None;
                            }
                            self.const_int_value_at(init_expr, decl_pos, 0)
                        });
                    let Some(v) = folded else {
                        let msg = format!(
                            "array length `{}` is not an integer constant — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                            name
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                                "why: an array length must be a non-negative integer constant in Go (issue #201)".to_string(),
                                "fix: declare the `const` with a non-negative integer value, or declare a slice (`[]T`)".to_string(),
                            ]),
                        );
                        return;
                    };
                    if v < 0 {
                        let msg = format!(
                            "array length `{}` folds to negative constant `{}` — Go rejects this (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                            name, v, v
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg)
                                .with_notes(vec![
                                    "why: an array length must be a non-negative constant in Go (issue #201)".to_string(),
                                    "fix: declare the const with a non-negative value, or use a slice (`[]T`)".to_string(),
                                ]),
                        );
                    }
                    return;
                }
            }
            if self.consts.contains_key(name.as_str()) {
                if !self.file_const_is_valid_array_len(name, 0) {
                    let msg = format!(
                        "array length `{}` is not an integer constant — Go rejects this (`invalid array length`), so the emitted module would not build (HYB-E6004)",
                        name
                    );
                    self.diags.push(
                        Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                            "why: an array length must be a non-negative integer constant in Go (issue #201)".to_string(),
                            "fix: declare the `const` with a non-negative integer value, or declare a slice (`[]T`)".to_string(),
                        ]),
                    );
                    return;
                }
                // A file-level const: fold its value when we can prove it
                // negative (`const n = -4; var a [n]int`).
                if let Some(v) = self.const_value_of(name) {
                    if v < 0 {
                        let msg = format!(
                            "array length `{}` folds to negative constant `{}` — Go rejects this (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                            name, v, v
                        );
                        self.diags.push(
                            Diagnostic::error("HYB-E6004", self.path.clone(), span, msg)
                                .with_notes(vec![
                                    "why: an array length must be a non-negative constant in Go (issue #201)".to_string(),
                                    "fix: declare the const with a non-negative value, or use a slice (`[]T`)".to_string(),
                                ]),
                        );
                    }
                }
                return;
            }
            let msg = format!(
                "array length `{}` is not a constant — Go requires a constant array length (`invalid array length {}`), so the emitted module would not build (HYB-E6004)",
                name, name
            );
            self.diags.push(
                Diagnostic::error("HYB-E6004", self.path.clone(), span, msg).with_notes(vec![
                    "why: Go's array length is part of the type and must be a non-negative constant; a variable length would change the type (issue #201)".to_string(),
                    "fix: use a literal or file-level `const` length, or declare a slice (`[]T`) for a dynamic size".to_string(),
                ]),
            );
        }
        // 4) Anything else (calls, `pkg.CONST`, arithmetic over idents) —
        // fail open: those can be legal constant expressions in Go.
    }

    fn file_const_is_valid_array_len(&self, name: &str, depth: usize) -> bool {
        if depth > 32 {
            return false;
        }
        if let Some(b) = self.consts.get(name) {
            if !matches!(b.shape, TypeShape::Primitive | TypeShape::Unknown) {
                return false;
            }
        }
        let Some(d) = self.file.decls.iter().find_map(|d| match d {
            hy_syntax::Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        }) else {
            return true;
        };
        if let Some(ty) = &d.ty {
            if is_non_int_primitive_type(ty) {
                return false;
            }
        }
        if is_non_int_array_len_expr(&d.value) {
            return false;
        }
        if const_float_value(&d.value).is_some_and(|f| f.fract() != 0.0) {
            return false;
        }
        if let Expr::Ident { name: next, .. } = unwrap_parens(&d.value) {
            if next != name && self.consts.contains_key(next.as_str()) {
                return self.file_const_is_valid_array_len(next, depth + 1);
            }
        }
        self.const_int_value_at(&d.value, d.span.start, depth + 1)
            .is_some()
    }

    /// Fold the file-level `const` named `name` to an integer, when its
    /// declared value is a constant expression the folder understands.
    fn const_value_of(&self, name: &str) -> Option<i128> {
        self.const_value_of_depth(name, 0)
    }

    fn const_value_of_depth(&self, name: &str, depth: usize) -> Option<i128> {
        if depth > 32 {
            return None;
        }
        let d = self.file.decls.iter().find_map(|d| match d {
            hy_syntax::Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })?;
        if d.ty.as_ref().is_some_and(is_non_int_primitive_type) {
            return None;
        }
        self.const_int_value_at(&d.value, d.span.start, depth + 1)
    }

    fn const_int_value_at(&self, expr: &Expr, use_pos: hy_diag::Pos, depth: usize) -> Option<i128> {
        if depth > 32 {
            return None;
        }
        if let Some(v) = const_int_value(expr) {
            return Some(v);
        }
        if let Some(f) = const_float_value(expr) {
            if f.is_finite()
                && f.fract() == 0.0
                && f >= (i64::MIN as f64)
                && f <= (i64::MAX as f64)
            {
                return Some(f as i128);
            }
            return None;
        }
        match unwrap_parens(expr) {
            Expr::Ident {
                name,
                span: id_span,
            } => {
                if let Some((decl_ty, next_val, next_pos)) =
                    self.resolve_local_const_decl_at(name, id_span.start)
                {
                    if decl_ty.as_ref().is_some_and(is_non_int_primitive_type) {
                        return None;
                    }
                    if (next_pos.line, next_pos.col) != (use_pos.line, use_pos.col) {
                        return self.const_int_value_at(next_val, next_pos, depth + 1);
                    }
                    return None;
                }
                if self.consts.contains_key(name.as_str()) {
                    return self.const_value_of_depth(name, depth + 1);
                }
                None
            }
            Expr::Unary {
                op: hy_syntax::UnOp::Neg,
                operand,
                ..
            } => self
                .const_int_value_at(operand, use_pos, depth)
                .and_then(|v| v.checked_neg()),
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.const_int_value_at(lhs, use_pos, depth)?;
                let r = self.const_int_value_at(rhs, use_pos, depth)?;
                match op {
                    BinOp::Add => l.checked_add(r),
                    BinOp::Sub => l.checked_sub(r),
                    BinOp::Mul => l.checked_mul(r),
                    BinOp::Div if r != 0 => l.checked_div(r),
                    BinOp::Mod if r != 0 => l.checked_rem(r),
                    BinOp::BitOr => Some(l | r),
                    BinOp::BitAnd => Some(l & r),
                    BinOp::BitXor => Some(l ^ r),
                    BinOp::Shl if (0..128).contains(&r) => l.checked_shl(r as u32),
                    BinOp::Shr if (0..128).contains(&r) => l.checked_shr(r as u32),
                    _ => None,
                }
            }
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                let Expr::Ident { name: fn_name, .. } = unwrap_parens(callee) else {
                    return None;
                };
                if self.fns.contains_key(fn_name.as_str()) {
                    return None;
                }
                if fn_name == "len" {
                    return self.const_string_len_at(&args[0].value, use_pos, depth);
                }
                if self.is_int_conversion_name(fn_name) {
                    return self.const_int_value_at(&args[0].value, use_pos, depth);
                }
                None
            }
            _ => None,
        }
    }

    fn const_string_len_at(&self, expr: &Expr, use_pos: hy_diag::Pos, depth: usize) -> Option<i128> {
        if depth > 32 {
            return None;
        }
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: hy_syntax::LitKind::Str(s) | hy_syntax::LitKind::RawStr(s),
                ..
            } => Some(s.len() as i128),
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                let l = self.const_string_len_at(lhs, use_pos, depth)?;
                let r = self.const_string_len_at(rhs, use_pos, depth)?;
                l.checked_add(r)
            }
            Expr::Ident {
                name,
                span: id_span,
            } => {
                if let Some((_, next_val, next_pos)) =
                    self.resolve_local_const_decl_at(name, id_span.start)
                {
                    if (next_pos.line, next_pos.col) != (use_pos.line, use_pos.col) {
                        return self.const_string_len_at(next_val, next_pos, depth + 1);
                    }
                    return None;
                }
                let d = self.file.decls.iter().find_map(|d| match d {
                    hy_syntax::Decl::Const(c) if c.name == *name => Some(c),
                    _ => None,
                })?;
                self.const_string_len_at(&d.value, d.span.start, depth + 1)
            }
            _ => None,
        }
    }

    fn is_int_conversion_name(&self, name: &str) -> bool {
        if is_int_primitive_type_name(name) {
            return true;
        }
        if let Some(ty) = self.type_aliases.get(name) {
            let resolved = self.resolve_type_alias(ty);
            if let TypeExpr::Named { name: target, .. } = resolved {
                return is_int_primitive_type_name(&target);
            }
        }
        false
    }

    fn resolve_local_const_decl_at(
        &self,
        name: &str,
        use_pos: hy_diag::Pos,
    ) -> Option<(&'a Option<TypeExpr>, &'a Expr, hy_diag::Pos)> {
        for decl in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = decl {
                if span_contains(f.span, use_pos) {
                    let mut found = None;
                    if let Some(body) = &f.body {
                        Self::scan_block_for_local_const(body, name, use_pos, &mut found);
                    }
                    return found;
                }
            }
        }
        None
    }

    fn scan_block_for_local_const<'b>(
        block: &'b hy_syntax::Block,
        name: &str,
        use_pos: hy_diag::Pos,
        found: &mut Option<(&'b Option<TypeExpr>, &'b Expr, hy_diag::Pos)>,
    ) {
        if !span_contains(block.span, use_pos) {
            return;
        }
        for stmt in &block.stmts {
            if Self::stmt_contains_pos(stmt, use_pos) {
                Self::scan_stmt_for_local_const(stmt, name, use_pos, found);
                return;
            }
            if Self::stmt_ends_before(stmt, use_pos) {
                Self::apply_local_stmt_binding(stmt, name, found);
            }
        }
    }

    fn apply_local_stmt_binding<'b>(
        stmt: &'b Stmt,
        name: &str,
        found: &mut Option<(&'b Option<TypeExpr>, &'b Expr, hy_diag::Pos)>,
    ) {
        if let Stmt::Local(l) = stmt {
            if let Some(idx) = l.names.iter().position(|n| n == name) {
                if matches!(l.kind, hy_syntax::LocalKind::Const) {
                    if let Some(val) = l.values.get(idx) {
                        *found = Some((&l.ty, val, l.span.start));
                    } else {
                        *found = None;
                    }
                } else {
                    *found = None;
                }
            }
        }
    }

    fn scan_stmt_for_local_const<'b>(
        stmt: &'b Stmt,
        name: &str,
        use_pos: hy_diag::Pos,
        found: &mut Option<(&'b Option<TypeExpr>, &'b Expr, hy_diag::Pos)>,
    ) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    if span_contains(v.span(), use_pos) {
                        Self::scan_expr_for_local_const(v, name, use_pos, found);
                        return;
                    }
                }
            }
            Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
                for v in values {
                    if span_contains(v.span(), use_pos) {
                        Self::scan_expr_for_local_const(v, name, use_pos, found);
                        return;
                    }
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                if span_contains(expr.span(), use_pos) {
                    Self::scan_expr_for_local_const(expr, name, use_pos, found);
                }
            }
            Stmt::Send { chan, value, .. } => {
                if span_contains(chan.span(), use_pos) {
                    Self::scan_expr_for_local_const(chan, name, use_pos, found);
                } else if span_contains(value.span(), use_pos) {
                    Self::scan_expr_for_local_const(value, name, use_pos, found);
                }
            }
            Stmt::Task { callee, args, .. } => {
                if span_contains(callee.span(), use_pos) {
                    Self::scan_expr_for_local_const(callee, name, use_pos, found);
                    return;
                }
                for a in args {
                    if span_contains(a.value.span(), use_pos) {
                        Self::scan_expr_for_local_const(&a.value, name, use_pos, found);
                        return;
                    }
                }
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(init_stmt) = init {
                    if Self::stmt_contains_pos(init_stmt, use_pos) {
                        Self::scan_stmt_for_local_const(init_stmt, name, use_pos, found);
                        return;
                    }
                    if Self::stmt_ends_before(init_stmt, use_pos) {
                        Self::apply_local_stmt_binding(init_stmt, name, found);
                    }
                }
                if span_contains(cond.span(), use_pos) {
                    Self::scan_expr_for_local_const(cond, name, use_pos, found);
                    return;
                }
                if span_contains(then.span, use_pos) {
                    Self::scan_block_for_local_const(then, name, use_pos, found);
                    return;
                }
                if let Some(other) = otherwise {
                    if Self::stmt_contains_pos(other, use_pos) {
                        Self::scan_stmt_for_local_const(other, name, use_pos, found);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => {
                        if span_contains(cond.span(), use_pos) {
                            Self::scan_expr_for_local_const(cond, name, use_pos, found);
                            return;
                        }
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        if span_contains(lo.span(), use_pos) {
                            Self::scan_expr_for_local_const(lo, name, use_pos, found);
                            return;
                        }
                        if span_contains(hi.span(), use_pos) {
                            Self::scan_expr_for_local_const(hi, name, use_pos, found);
                            return;
                        }
                        if var == name {
                            *found = None;
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        if span_contains(expr.span(), use_pos) {
                            Self::scan_expr_for_local_const(expr, name, use_pos, found);
                            return;
                        }
                        if vars.iter().any(|v| v == name) {
                            *found = None;
                        }
                    }
                    ForKind::Infinite => {}
                }
                if span_contains(body.span, use_pos) {
                    Self::scan_block_for_local_const(body, name, use_pos, found);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                Self::scan_block_for_local_const(body, name, use_pos, found);
            }
            Stmt::Match { subject, arms, .. } => {
                if span_contains(subject.span(), use_pos) {
                    Self::scan_expr_for_local_const(subject, name, use_pos, found);
                    return;
                }
                for arm in arms {
                    let in_guard = arm
                        .guard
                        .as_ref()
                        .is_some_and(|g| span_contains(g.span(), use_pos));
                    let in_body = span_contains(arm.body.span, use_pos);
                    if in_guard || in_body {
                        if let hy_syntax::MatchPattern::Path {
                            binding: Some(b), ..
                        } = &arm.pattern
                        {
                            if b == name {
                                *found = None;
                            }
                        }
                        if let Some(g) = &arm.guard {
                            if in_guard {
                                Self::scan_expr_for_local_const(g, name, use_pos, found);
                                return;
                            }
                        }
                        if in_body {
                            Self::scan_block_for_local_const(&arm.body, name, use_pos, found);
                        }
                        return;
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            if span_contains(chan.span(), use_pos) {
                                Self::scan_expr_for_local_const(chan, name, use_pos, found);
                                return;
                            }
                            if span_contains(value.span(), use_pos) {
                                Self::scan_expr_for_local_const(value, name, use_pos, found);
                                return;
                            }
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            if span_contains(chan.span(), use_pos) {
                                Self::scan_expr_for_local_const(chan, name, use_pos, found);
                                return;
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    if span_contains(arm.body.span, use_pos) {
                        if let SelectArmKind::Recv { names, .. } = &arm.kind {
                            if names.iter().any(|n| n == name) {
                                *found = None;
                            }
                        }
                        Self::scan_block_for_local_const(&arm.body, name, use_pos, found);
                        return;
                    }
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn scan_expr_for_local_const<'b>(
        expr: &'b Expr,
        name: &str,
        use_pos: hy_diag::Pos,
        found: &mut Option<(&'b Option<TypeExpr>, &'b Expr, hy_diag::Pos)>,
    ) {
        if !span_contains(expr.span(), use_pos) {
            return;
        }
        match expr {
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    if span_contains(p.ty.span(), use_pos) {
                        return;
                    }
                }
                for r in results {
                    if span_contains(r.ty.span(), use_pos) {
                        return;
                    }
                }
                if params.iter().any(|p| p.name == name)
                    || results.iter().any(|r| r.name.as_deref() == Some(name))
                {
                    *found = None;
                }
                if span_contains(body.span, use_pos) {
                    Self::scan_block_for_local_const(body, name, use_pos, found);
                }
            }
            Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
                Self::scan_expr_for_local_const(lhs, name, use_pos, found);
                Self::scan_expr_for_local_const(rhs, name, use_pos, found);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                Self::scan_expr_for_local_const(operand, name, use_pos, found);
            }
            Expr::Slice { base, lo, hi, .. } => {
                Self::scan_expr_for_local_const(base, name, use_pos, found);
                if let Some(lo) = lo {
                    Self::scan_expr_for_local_const(lo, name, use_pos, found);
                }
                if let Some(hi) = hi {
                    Self::scan_expr_for_local_const(hi, name, use_pos, found);
                }
            }
            Expr::Call { callee, args, .. } => {
                Self::scan_expr_for_local_const(callee, name, use_pos, found);
                for a in args {
                    Self::scan_expr_for_local_const(&a.value, name, use_pos, found);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    Self::scan_expr_for_local_const(&f.value, name, use_pos, found);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    Self::scan_expr_for_local_const(i, name, use_pos, found);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    Self::scan_expr_for_local_const(k, name, use_pos, found);
                    Self::scan_expr_for_local_const(v, name, use_pos, found);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        Self::scan_expr_for_local_const(e, name, use_pos, found);
                    }
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn stmt_span(stmt: &Stmt) -> Span {
        match stmt {
            Stmt::Local(l) => l.span,
            Stmt::Assign { span, .. }
            | Stmt::Expr { span, .. }
            | Stmt::Send { span, .. }
            | Stmt::If { span, .. }
            | Stmt::For { span, .. }
            | Stmt::Return { span, .. }
            | Stmt::Task { span, .. }
            | Stmt::Managed { span, .. }
            | Stmt::Match { span, .. }
            | Stmt::Defer { span, .. }
            | Stmt::Break { span, .. }
            | Stmt::Continue { span, .. }
            | Stmt::Block { span, .. }
            | Stmt::Select { span, .. } => *span,
        }
    }

    fn stmt_contains_pos(stmt: &Stmt, pos: hy_diag::Pos) -> bool {
        span_contains(Self::stmt_span(stmt), pos)
    }

    fn stmt_ends_before(stmt: &Stmt, pos: hy_diag::Pos) -> bool {
        let s = Self::stmt_span(stmt);
        (s.end.line, s.end.col) <= (pos.line, pos.col)
    }
}

fn span_contains(span: Span, pos: hy_diag::Pos) -> bool {
    (span.start.line, span.start.col) <= (pos.line, pos.col)
        && (pos.line, pos.col) <= (span.end.line, span.end.col)
}

fn is_non_int_primitive_type(ty: &TypeExpr) -> bool {
    matches!(
        ty,
        TypeExpr::Named { name, .. }
            if matches!(
                name.as_str(),
                "bool" | "string" | "float32" | "float64" | "error"
            )
    )
}

fn is_non_int_array_len_expr(expr: &Expr) -> bool {
    match unwrap_parens(expr) {
        Expr::Lit {
            kind:
                hy_syntax::LitKind::Bool(_)
                | hy_syntax::LitKind::Str(_)
                | hy_syntax::LitKind::RawStr(_),
            ..
        } => true,
        Expr::Unary {
            op: hy_syntax::UnOp::Not,
            ..
        } => true,
        Expr::Binary {
            op:
                BinOp::Eq
                | BinOp::Ne
                | BinOp::Lt
                | BinOp::Le
                | BinOp::Gt
                | BinOp::Ge
                | BinOp::And
                | BinOp::Or,
            ..
        } => true,
        Expr::Interp { .. }
        | Expr::StructLit { .. }
        | Expr::SliceLit { .. }
        | Expr::MapLit { .. }
        | Expr::FnLit { .. }
        | Expr::AddrOf { .. } => true,
        Expr::Call { callee, .. } => matches!(
            unwrap_parens(callee),
            Expr::Ident { name, .. }
                if matches!(name.as_str(), "bool" | "string" | "float32" | "float64")
        ),
        _ => false,
    }
}

fn is_int_primitive_type_name(name: &str) -> bool {
    matches!(
        name,
        "int"
            | "uint"
            | "uintptr"
            | "isize"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "byte"
            | "rune"
    )
}

/// Go's receiver rule: the base type must be a defined type — not a
/// predeclared primitive (int/string/…) and not an interface (`error`).
/// `byte`/`rune` are aliases of primitives, so they reject too.
fn is_primitive_receiver(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "string"
            | "int"
            | "uint"
            | "uintptr"
            | "isize"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "float32"
            | "float64"
            | "byte"
            | "rune"
            | "char"
            | "error"
    )
}
