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
//! Inside closure bodies, names the closure rebinds (parameters, locals,
//! loop/match/select binders) are skipped: the enclosing `FnState` does
//! not know their shapes, so an outer binding's shape would be wrong.

use std::collections::{HashMap, HashSet};

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::Diagnostic;
use hy_syntax::{Block, Expr, Param, Place, PlaceKind, Stmt, TypeExpr};

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
        let mut inner = unwrap_parens(value);
        while let Expr::Move { operand, .. } = inner {
            inner = unwrap_parens(operand);
        }
        let Some(operand) = as_addr_of(inner) else {
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
            if matches!(self.effective_place_shape(st, base), Some(TypeShape::Map(..))) {
                return None;
            }
        }
        self.place_unaddressable_at(st, p)
    }

    /// `Some(map element)` when `p` is provably not addressable in Go.
    fn place_unaddressable_at<'p>(&self, st: &FnState, p: &'p Place) -> Option<&'p Place> {
        match &p.kind {
            PlaceKind::Index { base, .. } => match self.effective_place_shape(st, base) {
                Some(TypeShape::Map(..)) => Some(p),
                Some(TypeShape::Array(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Field { base, .. } => match self.effective_place_shape(st, base) {
                Some(TypeShape::Struct(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Ident(_) | PlaceKind::AddrOf { .. } => None,
        }
    }

    fn expr_unassignable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        if let Expr::Index { base, .. } = unwrap_parens(e) {
            if matches!(self.effective_expr_shape(st, base), TypeShape::Map(..)) {
                return None;
            }
        }
        self.expr_unaddressable_at(st, e)
    }

    fn expr_unaddressable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        match unwrap_parens(e) {
            Expr::Index { base, .. } => match self.effective_expr_shape(st, base) {
                TypeShape::Map(..) => Some(e),
                TypeShape::Array(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            Expr::Field { base, .. } => match self.effective_expr_shape(st, base) {
                TypeShape::Struct(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            _ => None,
        }
    }

    fn effective_place_shape(&self, st: &FnState, p: &Place) -> Option<TypeShape> {
        if let Some(ty) = self.resolved_place_ty(st, p) {
            let s = self.shape_from_resolved_ty(&ty);
            if !matches!(s, TypeShape::Unknown) {
                return Some(s);
            }
        }
        self.place_shape(st, p)
    }

    fn effective_expr_shape(&self, st: &FnState, e: &Expr) -> TypeShape {
        if let Some(ty) = self.resolved_expr_ty(st, e) {
            let s = self.shape_from_resolved_ty(&ty);
            if !matches!(s, TypeShape::Unknown) {
                return s;
            }
        }
        self.shape_of_expr(st, e)
    }

    fn resolved_place_ty(&self, st: &FnState, p: &Place) -> Option<TypeExpr> {
        match &p.kind {
            PlaceKind::Ident(name) => self.lookup_binding(st, name).and_then(|b| b.ty.clone()).map(|t| self.normalize_type_expr(&t)),
            PlaceKind::Field { base, name } => self.resolve_struct_field_ty(&self.resolved_place_ty(st, base)?, name),
            PlaceKind::Index { base, .. } => self.resolve_index_elem_ty(&self.resolved_place_ty(st, base)?),
            PlaceKind::AddrOf { .. } => None,
        }
    }

    fn resolved_expr_ty(&self, st: &FnState, e: &Expr) -> Option<TypeExpr> {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => self.lookup_binding(st, name).and_then(|b| b.ty.clone()).map(|t| self.normalize_type_expr(&t)),
            Expr::Field { base, name, .. } => {
                let bty = self.resolved_expr_ty(st, base).or_else(|| self.ty_of_expr(st, base).map(|t| self.normalize_type_expr(&t)))?;
                self.resolve_struct_field_ty(&bty, name)
            }
            Expr::Index { base, .. } => {
                let bty = self.resolved_expr_ty(st, base).or_else(|| self.ty_of_expr(st, base).map(|t| self.normalize_type_expr(&t)))?;
                self.resolve_index_elem_ty(&bty)
            }
            other => self.ty_of_expr(st, other).map(|t| self.normalize_type_expr(&t)),
        }
    }

    fn normalize_type_expr(&self, ty: &TypeExpr) -> TypeExpr {
        let resolved = self.resolve_type_alias(ty);
        match resolved {
            TypeExpr::Named { ref name, span } => parse_named_type_str(name, span)
                .map(|p| if matches!(p, TypeExpr::Named { .. }) { p } else { self.normalize_type_expr(&p) })
                .unwrap_or(resolved),
            other => other,
        }
    }

    fn resolve_struct_field_ty(&self, base_ty: &TypeExpr, field_name: &str) -> Option<TypeExpr> {
        let norm = self.normalize_type_expr(base_ty);
        let target = match norm {
            TypeExpr::Pointer { to, .. } => self.normalize_type_expr(&to),
            other => other,
        };
        let TypeExpr::Named { name, span } = target else { return None };
        let (head, inner_opt) = split_generic_head(&name);
        let (_, raw_field_ty) = self.structs.get(head)?.iter().find(|(n, _)| n == field_name)?;
        let mut subst = HashMap::new();
        if let (Some(tparams), Some(inner)) = (self.struct_type_params.get(head), inner_opt) {
            let args = split_top_level_type_args(inner);
            if args.len() == tparams.len() {
                for (tp, arg_str) in tparams.iter().zip(args.into_iter()) {
                    if let Some(arg_ty) = parse_named_type_str(arg_str, span) {
                        subst.insert(tp.name.clone(), self.normalize_type_expr(&arg_ty));
                    }
                }
            }
        }
        Some(self.normalize_type_expr(&substitute_type_params(raw_field_ty, &subst)))
    }

    fn resolve_index_elem_ty(&self, base_ty: &TypeExpr) -> Option<TypeExpr> {
        let norm = self.normalize_type_expr(base_ty);
        let target = match norm {
            TypeExpr::Pointer { to, .. } => {
                let inner = self.normalize_type_expr(&to);
                if matches!(inner, TypeExpr::Array { .. }) { inner } else { return None }
            }
            other => other,
        };
        match target {
            TypeExpr::Map { value, .. } => Some(self.normalize_type_expr(&value)),
            TypeExpr::Slice { elem, .. } | TypeExpr::Array { elem, .. } => Some(self.normalize_type_expr(&elem)),
            _ => None,
        }
    }

    fn shape_from_resolved_ty(&self, ty: &TypeExpr) -> TypeShape {
        let norm = self.normalize_type_expr(ty);
        match &norm {
            TypeExpr::Map { key, value, .. } => TypeShape::Map(Box::new(self.shape_from_resolved_ty(key)), Box::new(self.shape_from_resolved_ty(value))),
            TypeExpr::Slice { elem, .. } => TypeShape::Slice(Box::new(self.shape_from_resolved_ty(elem))),
            TypeExpr::Array { elem, .. } => TypeShape::Array(Box::new(self.shape_from_resolved_ty(elem))),
            TypeExpr::Pointer { to, .. } => TypeShape::Pointer(Box::new(self.shape_from_resolved_ty(to))),
            TypeExpr::Named { name, .. } => {
                let (head, _) = split_generic_head(name);
                if self.structs.contains_key(head) { TypeShape::Struct(Some(head.to_string())) } else { self.shape_of_type(&norm) }
            }
            _ => self.shape_of_type(&norm),
        }
    }

    fn closure_rebinds(&self, name: &str) -> bool {
        self.closure_bound.iter().any(|s| s.contains(name))
    }

    pub(crate) fn push_closure_bound(&mut self, _params: &[Param], body: &Block) {
        let mut names: HashSet<String> = HashSet::new();
        block_binders(body, &mut names);
        self.closure_bound.push(names);
    }

    pub(crate) fn pop_closure_bound(&mut self) {
        self.closure_bound.pop();
    }
}

fn split_generic_head(name: &str) -> (&str, Option<&str>) {
    let name = name.trim();
    match name.find('[') {
        Some(i) if name.ends_with(']') => (&name[..i], Some(&name[i + 1..name.len() - 1])),
        _ => (name, None),
    }
}

fn split_top_level_type_args(s: &str) -> Vec<&str> {
    let (mut out, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    for (i, ch) in s.char_indices() {
        match ch {
            '[' | '(' => depth += 1,
            ']' | ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(s[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = s[start..].trim();
    if !tail.is_empty() { out.push(tail); }
    out
}

fn parse_named_type_str(raw: &str, span: hy_diag::Span) -> Option<TypeExpr> {
    let s = raw.trim();
    if s.is_empty() { return None; }
    if let Some(rest) = s.strip_prefix('*') {
        return Some(TypeExpr::Pointer { to: Box::new(parse_named_type_str(rest, span)?), span });
    }
    if let Some(rest) = s.strip_prefix("[]") {
        return Some(TypeExpr::Slice { elem: Box::new(parse_named_type_str(rest, span)?), span });
    }
    if let Some(rest) = s.strip_prefix("map[") {
        let close = find_matching_bracket(rest)?;
        return Some(TypeExpr::Map {
            key: Box::new(parse_named_type_str(&rest[..close], span)?),
            value: Box::new(parse_named_type_str(&rest[close + 1..], span)?),
            span,
        });
    }
    if let Some(rest) = s.strip_prefix('[') {
        let close = find_matching_bracket(rest)?;
        return Some(TypeExpr::Array { len: None, elem: Box::new(parse_named_type_str(&rest[close + 1..], span)?), span });
    }
    Some(TypeExpr::Named { name: s.to_string(), span })
}

fn find_matching_bracket(s: &str) -> Option<usize> {
    let mut depth = 1usize;
    for (i, ch) in s.char_indices() {
        if ch == '[' { depth += 1; } else if ch == ']' {
            depth -= 1;
            if depth == 0 { return Some(i); }
        }
    }
    None
}

fn substitute_type_params(ty: &TypeExpr, subst: &HashMap<String, TypeExpr>) -> TypeExpr {
    if subst.is_empty() { return ty.clone(); }
    match ty {
        TypeExpr::Named { name, span } => {
            if let Some(rep) = subst.get(name.as_str()) { return rep.clone(); }
            let (head, Some(inner)) = split_generic_head(name) else { return ty.clone() };
            let args: Vec<String> = split_top_level_type_args(inner).into_iter().map(|arg| {
                subst.get(arg).map(|r| r.render())
                    .or_else(|| parse_named_type_str(arg, *span).map(|p| substitute_type_params(&p, subst).render()))
                    .unwrap_or_else(|| arg.to_string())
            }).collect();
            TypeExpr::Named { name: format!("{}[{}]", head, args.join(", ")), span: *span }
        }
        TypeExpr::Pointer { to, span } => TypeExpr::Pointer { to: Box::new(substitute_type_params(to, subst)), span: *span },
        TypeExpr::Slice { elem, span } => TypeExpr::Slice { elem: Box::new(substitute_type_params(elem, subst)), span: *span },
        TypeExpr::Array { len, elem, span } => TypeExpr::Array { len: len.clone(), elem: Box::new(substitute_type_params(elem, subst)), span: *span },
        TypeExpr::Map { key, value, span } => TypeExpr::Map {
            key: Box::new(substitute_type_params(key, subst)),
            value: Box::new(substitute_type_params(value, subst)),
            span: *span,
        },
        TypeExpr::Chan { dir, elem, span } => TypeExpr::Chan { dir: *dir, elem: Box::new(substitute_type_params(elem, subst)), span: *span },
        TypeExpr::Option { elem, span } => TypeExpr::Option { elem: Box::new(substitute_type_params(elem, subst)), span: *span },
        TypeExpr::Fn { params, results, span } => TypeExpr::Fn {
            params: params.iter().map(|p| Param { name: p.name.clone(), ty: substitute_type_params(&p.ty, subst), span: p.span }).collect(),
            results: results.iter().map(|r| substitute_type_params(r, subst)).collect(),
            span: *span,
        },
    }
}

fn expr_root(e: &Expr) -> Option<&str> {
    match unwrap_parens(e) {
        Expr::Ident { name, .. } => Some(name),
        Expr::Index { base, .. } | Expr::Field { base, .. } => expr_root(base),
        _ => None,
    }
}

/// Every name a block (and its nested statement blocks) binds. Nested
/// closures are not entered: their binders shadow only inside themselves.
fn block_binders(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        stmt_binders(s, out);
    }
}

fn stmt_binders(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Local(l) => {
            if !l.values.is_empty() && l.names.len() != l.values.len() {
                out.extend(l.names.iter().cloned());
            }
        }
        Stmt::If { init, then, otherwise, .. } => {
            if let Some(i) = init {
                stmt_binders(i, out);
            }
            block_binders(then, out);
            if let Some(o) = otherwise {
                stmt_binders(o, out);
            }
        }
        Stmt::For { body, .. } | Stmt::Managed { body, .. } => block_binders(body, out),
        Stmt::Match { arms, .. } => {
            for arm in arms {
                block_binders(&arm.body, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                block_binders(&arm.body, out);
            }
        }
        Stmt::Block { block, .. } => block_binders(block, out),
        _ => {}
    }
}
