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
use hy_syntax::{Block, Expr, ForKind, MatchPattern, Param, Place, PlaceKind, SelectArmKind, Stmt};

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
        let Some(operand) = as_addr_of(value) else {
            return;
        };
        let operand = unwrap_parens(operand);
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
            if matches!(self.place_shape(st, base), Some(TypeShape::Map(..))) {
                return None;
            }
        }
        self.place_unaddressable_at(st, p)
    }

    /// `Some(map element)` when `p` is provably not addressable in Go.
    fn place_unaddressable_at<'p>(&self, st: &FnState, p: &'p Place) -> Option<&'p Place> {
        match &p.kind {
            PlaceKind::Index { base, .. } => match self.place_shape(st, base) {
                Some(TypeShape::Map(..)) | Some(TypeShape::Str) => Some(p),
                Some(TypeShape::Array(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Field { base, .. } => match self.place_shape(st, base) {
                Some(TypeShape::Struct(_)) => self.place_unaddressable_at(st, base),
                _ => None,
            },
            PlaceKind::Ident(_) | PlaceKind::AddrOf { .. } => None,
        }
    }

    fn expr_unassignable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        if let Expr::Index { base, .. } = unwrap_parens(e) {
            if matches!(self.shape_of_expr(st, base), TypeShape::Map(..)) {
                return None;
            }
        }
        self.expr_unaddressable_at(st, e)
    }

    fn expr_unaddressable_at<'e>(&self, st: &FnState, e: &'e Expr) -> Option<&'e Expr> {
        let e = unwrap_parens(e);
        match e {
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(..) | TypeShape::Str => Some(e),
                TypeShape::Array(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            Expr::Field { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Struct(_) => self.expr_unaddressable_at(st, base),
                _ => None,
            },
            Expr::Call { .. }
                if matches!(
                    self.shape_of_expr(st, e),
                    TypeShape::Struct(_) | TypeShape::Array(_)
                ) =>
            {
                Some(e)
            }
            _ => None,
        }
    }

    fn closure_rebinds(&self, name: &str) -> bool {
        self.closure_bound.iter().any(|s| s.contains(name))
    }

    /// Entering a closure body in the emit-guard deep walk.
    pub(crate) fn push_closure_bound(&mut self, params: &[Param], body: &Block) {
        let mut names: HashSet<String> = params.iter().map(|p| p.name.clone()).collect();
        block_binders(body, &mut names);
        self.closure_bound.push(names);
    }

    pub(crate) fn pop_closure_bound(&mut self) {
        self.closure_bound.pop();
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
        Stmt::Local(l) => out.extend(l.names.iter().cloned()),
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
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Range { var, .. } => {
                    out.insert(var.clone());
                }
                ForKind::Iterate { vars, .. } => out.extend(vars.iter().cloned()),
                ForKind::While { .. } | ForKind::Infinite => {}
            }
            block_binders(body, out);
        }
        Stmt::Match { arms, .. } => {
            for arm in arms {
                if let MatchPattern::Path {
                    binding: Some(n), ..
                } = &arm.pattern
                {
                    out.insert(n.clone());
                }
                block_binders(&arm.body, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                if let SelectArmKind::Recv { names, .. } = &arm.kind {
                    out.extend(names.iter().cloned());
                }
                block_binders(&arm.body, out);
            }
        }
        Stmt::Managed { body, .. } => block_binders(body, out),
        Stmt::Block { block, .. } => block_binders(block, out),
        _ => {}
    }
}
