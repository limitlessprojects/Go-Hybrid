//! addressable.rs — an accepted store target must be a Go-assignable place.
//!
//! Go map elements are not addressable: `m[k] = v` is legal (a map index
//! expression is assignable), but a store INTO part of the element —
//! `m[k].f = v`, `m[k].f += 1`, `m[k].f++`, `m[k][i] = v` on an array
//! element, `m[k].arr[i] = v`, `b.accts[k].in.n = v` — is rejected by Go
//! (`cannot assign to struct field m[k].f in map` / `neither addressable
//! nor a map index expression`). The same holds for the store-back place
//! of an O8 loan the emitter lowers to read-modify-write: `n := &m[k].f`
//! becomes `n := m[k].f` … `m[k].f = n`, which cannot build either.
//!
//! These shapes used to pass `hyb check` with 0 diagnostics and emit Go
//! that fails `go build` — the accepted-implies-buildable contract (rule
//! BUILDABLE). They now reject as HYB-E2010.
//!
//! Only PROVEN shapes reject: the base must resolve to a map, and every
//! hop between it and the store must resolve to a struct value or an
//! array value. A pointer-typed or unresolved hop stays legal (Go
//! auto-dereferences pointers, and the checker has no full type system).

use std::collections::{HashMap, HashSet};

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::Diagnostic;
use hy_syntax::{Block, Decl, Expr, Param, Place, PlaceKind, Stmt, TypeExpr};

impl<'a> Checker<'a> {
    /// `target` of an assignment (any operator, including `++`/`--`).
    pub(crate) fn check_assign_target_addressable(&mut self, st: &FnState, target: &Place) {
        if self.closure_rebinds(target.root()) {
            return;
        }
        let Some(elem) = self.place_unassignable_at(st, target) else {
            return;
        };
        let elem = elem.render();
        let msg = format!(
            "cannot assign to `{}` — it is part of the map element `{}`, and a Go map element is not addressable (HYB-E2010)",
            target.render(),
            elem
        );
        self.push_unaddressable(msg, &elem, target.span);
    }

    /// `&operand` bound or re-bound as an O8 loan: the emitter stores the
    /// copy back into `operand` at the loan's last use, so `operand` must
    /// be assignable in Go.
    pub(crate) fn check_slot_loan_operand_addressable(&mut self, st: &FnState, value: &Expr) {
        let unwrapped_val = unwrap_move_and_parens(value);
        let Some(operand) = as_addr_of(unwrapped_val) else {
            return;
        };
        if !matches!(operand, Expr::Index { .. } | Expr::Field { .. }) {
            return;
        }
        if expr_root(operand).is_some_and(|r| self.closure_rebinds(r)) {
            return;
        }
        let Some(elem) = self.expr_unassignable_at(st, operand) else {
            return;
        };
        let elem = elem.render();
        let msg = format!(
            "`&{}` cannot be a loan — the loan is stored back into `{}`, which is part of the map element `{}`, and a Go map element is not addressable (HYB-E2010)",
            operand.render(),
            operand.render(),
            elem
        );
        self.push_unaddressable(msg, &elem, value.span());
    }

    fn push_unaddressable(&mut self, msg: String, elem: &str, span: hy_diag::Span) {
        let notes = vec![
            "why: Go rejects a store into part of a map element (`cannot assign to struct field … in map` / `neither addressable nor a map index expression`), so an accepted Hybrid program would emit unbuildable Go".to_string(),
            format!(
                "map reads are copies (O9): take the whole slot as an O8 loan — `n := &{}` — and store through `n`, or copy the element out, modify it, and assign it back with `m[k] = …`",
                elem
            ),
        ];
        self.diags
            .push(Diagnostic::error("HYB-E2010", self.path.clone(), span, msg).with_notes(notes));
    }

    /// `Some(map element)` when `p` is provably not a Go assignment target.
    /// A bare map index `m[k]` is assignable even though it is not
    /// addressable.
    fn place_unassignable_at<'p>(&self, st: &FnState, p: &'p Place) -> Option<&'p Place> {
        if let PlaceKind::Index { base, .. } = &p.kind {
            if matches!(self.resolved_place_shape(st, base), Some(TypeShape::Map(..))) {
                return None;
            }
        }
        self.place_unaddressable_at(st, p)
    }

    /// `Some(map element)` when `p` is provably not addressable in Go.
    fn place_unaddressable_at<'p>(&self, st: &FnState, p: &'p Place) -> Option<&'p Place> {
        match &p.kind {
            PlaceKind::Index { base, .. } => match self.resolved_place_shape(st, base) {
                Some(TypeShape::Map(..)) => Some(p),
                Some(TypeShape::Array(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Field { base, .. } => match self.resolved_place_shape(st, base) {
                Some(TypeShape::Struct(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Ident(_) | PlaceKind::AddrOf { .. } => None,
        }
    }

    fn expr_unassignable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        if let Expr::Index { base, .. } = unwrap_parens(e) {
            if matches!(self.resolved_expr_shape(st, base), TypeShape::Map(..)) {
                return None;
            }
        }
        self.expr_unaddressable_at(st, e)
    }

    fn expr_unaddressable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        match unwrap_parens(e) {
            Expr::Index { base, .. } => match self.resolved_expr_shape(st, base) {
                TypeShape::Map(..) => Some(e),
                TypeShape::Array(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            Expr::Field { base, .. } => match self.resolved_expr_shape(st, base) {
                TypeShape::Struct(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            _ => None,
        }
    }

    fn resolved_place_shape(&self, st: &FnState, p: &Place) -> Option<TypeShape> {
        if let Some(ty) = self.resolved_place_ty(st, p) {
            let resolved = self.resolve_type_alias(&ty);
            let s = self.shape_of_type(&resolved);
            if !matches!(s, TypeShape::Unknown) {
                return Some(s);
            }
        }
        self.place_shape(st, p)
    }

    fn resolved_expr_shape(&self, st: &FnState, e: &Expr) -> TypeShape {
        if let Some(ty) = self.resolved_expr_ty(st, e) {
            let resolved = self.resolve_type_alias(&ty);
            let s = self.shape_of_type(&resolved);
            if !matches!(s, TypeShape::Unknown) {
                return s;
            }
        }
        self.shape_of_expr(st, e)
    }

    fn resolved_place_ty(&self, st: &FnState, p: &Place) -> Option<TypeExpr> {
        match &p.kind {
            PlaceKind::Ident(name) => self.lookup_binding(st, name).and_then(|b| b.ty.clone()),
            PlaceKind::Field { base, name } => {
                let base_ty = self.resolved_place_ty(st, base)?;
                self.resolve_struct_field_ty(&base_ty, name)
            }
            PlaceKind::Index { base, .. } => {
                let base_ty = self.resolved_place_ty(st, base)?;
                self.resolve_index_elem_ty(&base_ty)
            }
            PlaceKind::AddrOf { base } => self.resolved_place_ty(st, base),
        }
    }

    fn resolved_expr_ty(&self, st: &FnState, e: &Expr) -> Option<TypeExpr> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self
                .lookup_binding(st, name)
                .and_then(|b| b.ty.clone())
                .or_else(|| self.ty_of_expr(st, e)),
            Expr::Field { base, name, .. } => {
                let base_ty = self.resolved_expr_ty(st, base)?;
                self.resolve_struct_field_ty(&base_ty, name)
                    .or_else(|| self.ty_of_expr(st, e))
            }
            Expr::Index { base, .. } => {
                let base_ty = self.resolved_expr_ty(st, base)?;
                self.resolve_index_elem_ty(&base_ty)
                    .or_else(|| self.ty_of_expr(st, e))
            }
            Expr::Move { operand, .. } | Expr::Try { operand, .. } => {
                self.resolved_expr_ty(st, operand)
            }
            Expr::Call { callee, .. } => {
                if let Expr::Index { base, index, .. } = unwrap_parens(callee) {
                    if let Some(ret) = self.resolve_generic_index_call_ty(st, base, index) {
                        return Some(ret);
                    }
                }
                self.ty_of_expr(st, e)
            }
            _ => self.ty_of_expr(st, e),
        }
    }

    fn resolve_index_elem_ty(&self, base_ty: &TypeExpr) -> Option<TypeExpr> {
        let mut cur = self.resolve_type_alias(base_ty);
        while let TypeExpr::Pointer { to, .. } = cur {
            cur = self.resolve_type_alias(&to);
        }
        match cur {
            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => Some(*elem),
            TypeExpr::Map { value, .. } => Some(*value),
            _ => None,
        }
    }

    fn resolve_struct_field_ty(&self, base_ty: &TypeExpr, field_name: &str) -> Option<TypeExpr> {
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

    fn resolve_generic_index_call_ty(
        &self,
        st: &FnState,
        base: &Expr,
        index: &Expr,
    ) -> Option<TypeExpr> {
        let arg_ty = parse_single_type_expr(&index.render())?;
        if let Expr::Ident { name, .. } = unwrap_parens(base) {
            if self.lookup_binding(st, name).is_none() {
                let f = self.fns.get(name.as_str())?;
                let ret = &f.results.first()?.ty;
                if let Some(tp) = f.type_params.first() {
                    let mut subst = HashMap::new();
                    subst.insert(tp.name.clone(), arg_ty);
                    return Some(subst_type_expr(ret, &subst));
                }
                return Some(ret.clone());
            }
        }
        None
    }

    fn closure_rebinds(&self, name: &str) -> bool {
        self.closure_bound.iter().any(|s| s.contains(name))
    }

    /// Entering a closure body in the emit-guard deep walk.
    /// `emit_guard.rs` maintains a closure-scoped `FnState` (`cst`) that
    /// accurately binds closure parameters, named results, single-value /
    /// uninitialized `Stmt::Local` declarations, `for` range/iterate vars,
    /// `match` arm bindings, and `select` recv bindings in lexical order.
    /// Only multi-value `Stmt::Local` declarations (`a, b := f()`) inside a
    /// closure body are not bound into `cst` by `bind_closure_local_for_emit_guard`,
    /// so only those untracked multi-value local binders need to be skipped here.
    pub(crate) fn push_closure_bound(&mut self, _params: &[Param], body: &Block) {
        let mut names: HashSet<String> = HashSet::new();
        untracked_closure_binders(body, &mut names);
        self.closure_bound.push(names);
    }

    pub(crate) fn pop_closure_bound(&mut self) {
        self.closure_bound.pop();
    }
}

fn unwrap_move_and_parens(mut e: &Expr) -> &Expr {
    loop {
        match e {
            Expr::Paren { inner, .. } => e = inner,
            Expr::Move { operand, .. } => e = operand,
            _ => return e,
        }
    }
}

fn expr_root(e: &Expr) -> Option<&str> {
    match unwrap_parens(e) {
        Expr::Ident { name, .. } => Some(name),
        Expr::Index { base, .. } | Expr::Field { base, .. } => expr_root(base),
        _ => None,
    }
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

fn untracked_closure_binders(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        untracked_stmt_binders(s, out);
    }
}

fn untracked_stmt_binders(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Local(l) => {
            if !l.values.is_empty() && l.names.len() != l.values.len() {
                out.extend(l.names.iter().cloned());
            }
        }
        Stmt::If {
            init,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                untracked_stmt_binders(i, out);
            }
            untracked_closure_binders(then, out);
            if let Some(o) = otherwise {
                untracked_stmt_binders(o, out);
            }
        }
        Stmt::For { body, .. } => untracked_closure_binders(body, out),
        Stmt::Match { arms, .. } => {
            for arm in arms {
                untracked_closure_binders(&arm.body, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                untracked_closure_binders(&arm.body, out);
            }
        }
        Stmt::Managed { body, .. } => untracked_closure_binders(body, out),
        Stmt::Block { block, .. } => untracked_closure_binders(block, out),
        _ => {}
    }
}
