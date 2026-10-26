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

use std::collections::HashSet;

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::Diagnostic;
use hy_syntax::{Block, Expr, Param, Place, PlaceKind, Stmt};

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

    /// `&operand` bound or re-bound as an O8 loan (or embedded inside an
    /// initializer / assignment RHS): the emitter stores the copy back into
    /// `operand` or emits `&operand` directly, so `operand` must be assignable
    /// and addressable in Go.
    pub(crate) fn check_slot_loan_operand_addressable(&mut self, st: &FnState, value: &Expr) {
        if let Some(operand) = as_addr_of(value) {
            if matches!(operand, Expr::Index { .. } | Expr::Field { .. })
                && !expr_root(operand).is_some_and(|r| self.closure_rebinds(r))
            {
                if let Some(elem) = self.expr_unassignable_at(st, operand) {
                    let elem = elem.render();
                    let msg = format!(
                        "`&{}` cannot be a loan — the loan is stored back into `{}`, which is part of the map element `{}`, and a Go map element is not addressable (HYB-E2010)",
                        operand.render(),
                        operand.render(),
                        elem
                    );
                    self.push_unaddressable(msg, &elem, value.span());
                }
            }
            self.check_subexpr_slot_loans_addressable(st, operand);
            return;
        }
        self.check_subexpr_slot_loans_addressable(st, value);
    }

    fn check_subexpr_slot_loans_addressable(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::Paren { inner: operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Receive { chan: operand, .. }
            | Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Field { base: operand, .. } => {
                self.check_slot_loan_operand_addressable(st, operand);
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_slot_loan_operand_addressable(st, lhs);
                self.check_slot_loan_operand_addressable(st, rhs);
            }
            Expr::Call { callee, args, .. } => {
                self.check_slot_loan_operand_addressable(st, callee);
                for a in args {
                    self.check_slot_loan_operand_addressable(st, &a.value);
                }
            }
            Expr::Index { base, index, .. } => {
                self.check_slot_loan_operand_addressable(st, base);
                self.check_slot_loan_operand_addressable(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_slot_loan_operand_addressable(st, base);
                if let Some(l) = lo {
                    self.check_slot_loan_operand_addressable(st, l);
                }
                if let Some(h) = hi {
                    self.check_slot_loan_operand_addressable(st, h);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_slot_loan_operand_addressable(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_slot_loan_operand_addressable(st, i);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_slot_loan_operand_addressable(st, k);
                    self.check_slot_loan_operand_addressable(st, v);
                }
            }
            // Closure bodies (`Expr::FnLit`) are walked separately with their inner `cst` in `emit_guard.rs`.
            _ => {}
        }
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
            if self
                .addr_place_shape(st, base)
                .is_some_and(|sh| is_map_or_ptr_to_map(&sh))
            {
                return None;
            }
        }
        self.place_unaddressable_at(st, p)
    }

    /// `Some(map element)` when `p` is provably not addressable in Go.
    fn place_unaddressable_at<'p>(&self, st: &FnState, p: &'p Place) -> Option<&'p Place> {
        match &p.kind {
            PlaceKind::Index { base, .. } => match self.addr_place_shape(st, base) {
                Some(sh) if is_map_or_ptr_to_map(&sh) => Some(p),
                Some(TypeShape::Array(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Field { base, .. } => match self.addr_place_shape(st, base) {
                Some(TypeShape::Struct(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::AddrOf { base } => self.place_unaddressable_at(st, base),
            PlaceKind::Ident(_) => None,
        }
    }

    fn expr_unassignable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        if let Expr::Index { base, .. } = unwrap_parens(e) {
            if is_map_or_ptr_to_map(&self.addr_expr_shape(st, base)) {
                return None;
            }
        }
        self.expr_unaddressable_at(st, e)
    }

    fn expr_unaddressable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        match unwrap_parens(e) {
            Expr::Index { base, .. } => match self.addr_expr_shape(st, base) {
                sh if is_map_or_ptr_to_map(&sh) => Some(e),
                TypeShape::Array(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            Expr::Field { base, .. } => match self.addr_expr_shape(st, base) {
                TypeShape::Struct(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            _ => None,
        }
    }

    fn addr_place_shape(&self, st: &FnState, p: &Place) -> Option<TypeShape> {
        match &p.kind {
            PlaceKind::Ident(n) => {
                let b = self.lookup_binding(st, n)?;
                if !matches!(b.shape, TypeShape::Unknown) {
                    return Some(b.shape.clone());
                }
                let root = crate::freeze::resolve_alias(st, n);
                if root != *n {
                    return self.lookup_binding(st, &root).map(|rb| rb.shape.clone());
                }
                Some(TypeShape::Unknown)
            }
            PlaceKind::Field { base, name } => {
                let b = self.addr_place_shape(st, base)?;
                match self.field_shape(&b, name) {
                    TypeShape::Unknown => None,
                    sh => Some(sh),
                }
            }
            PlaceKind::Index { base, .. } => match self.addr_place_shape(st, base)? {
                TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => Some(*v),
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => Some(*v),
                    _ => None,
                },
                _ => None,
            },
            PlaceKind::AddrOf { base } => self
                .addr_place_shape(st, base)
                .map(|inner| TypeShape::Pointer(Box::new(inner))),
        }
    }

    fn addr_expr_shape(&self, st: &FnState, e: &Expr) -> TypeShape {
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if !matches!(b.shape, TypeShape::Unknown) {
                        return b.shape.clone();
                    }
                    let root = crate::freeze::resolve_alias(st, name);
                    if root != *name {
                        if let Some(rb) = self.lookup_binding(st, &root) {
                            return rb.shape.clone();
                        }
                    }
                }
                self.shape_of_expr(st, e)
            }
            Expr::Field { base, name, .. } => {
                let b = self.addr_expr_shape(st, base);
                self.field_shape(&b, name)
            }
            Expr::Index { base, .. } => match self.addr_expr_shape(st, base) {
                TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => *v,
                TypeShape::Pointer(inner) => match *inner {
                    TypeShape::Map(_, v) | TypeShape::Slice(v) | TypeShape::Array(v) => *v,
                    _ => TypeShape::Unknown,
                },
                _ => TypeShape::Unknown,
            },
            _ => self.shape_of_expr(st, e),
        }
    }

    fn closure_rebinds(&self, name: &str) -> bool {
        self.closure_bound.iter().any(|s| s.contains(name))
    }

    /// Entering a closure body in the emit-guard deep walk.
    /// `emit_guard.rs` binds parameters, named results, 1-to-1/var locals,
    /// `if` inits, and `for`/`match`/`select` binders into `cst` in lexical
    /// scope order; only multi-value single-RHS local declarations that are
    /// not bound in `cst` need a conservative skip.
    pub(crate) fn push_closure_bound(&mut self, _params: &[Param], body: &Block) {
        let mut names = HashSet::new();
        block_binders(body, &mut names);
        self.closure_bound.push(names);
    }

    pub(crate) fn pop_closure_bound(&mut self) {
        self.closure_bound.pop();
    }
}

fn is_map_or_ptr_to_map(shape: &TypeShape) -> bool {
    match shape {
        TypeShape::Map(..) => true,
        TypeShape::Pointer(inner) => matches!(&**inner, TypeShape::Map(..)),
        _ => false,
    }
}

fn expr_root(e: &Expr) -> Option<&str> {
    match unwrap_parens(e) {
        Expr::Ident { name, .. } => Some(name),
        Expr::Index { base, .. } | Expr::Field { base, .. } => expr_root(base),
        _ => None,
    }
}

/// Multi-value single-RHS local declarations inside `b` (and its nested
/// statement blocks) whose shapes are not bound in the closure's `FnState`.
fn block_binders(b: &Block, out: &mut HashSet<String>) {
    for s in &b.stmts {
        stmt_binders(s, out);
    }
}

fn stmt_binders(s: &Stmt, out: &mut HashSet<String>) {
    match s {
        Stmt::Local(l) => {
            if !l.values.is_empty() && l.values.len() != l.names.len() {
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
