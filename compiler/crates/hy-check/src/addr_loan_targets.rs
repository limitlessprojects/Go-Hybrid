//! addr_loan_targets.rs — Issue #3752 (T24 / O8 HYB-E3104):
//!   1. `PlaceKind::AddrOf` assignment targets (`&q = &other`, `&q.val = &x`,
//!      `&q.arr[0] = &x`, `&m["k"] = &other`, `&m["k"].val = &x`).
//!   2. Mixed-target parallel assignment re-binding a live `slot_loan` to
//!      `&slice[i]` or `&arr[i]` (`sink[0], q = 1, &xs[0]`,
//!      `bx.tag, q = 1, &arr[0]`).
//!   3. `move`-wrapped operands inside `&(...)` (`&(move m["k"])`,
//!      `&(move m["k"].val)`, `&(move q)`).
//!   4. Loop-carried `slot_loan` in `ForKind::While` conditions and closure
//!      loops, and returning a value-updated or aliased `slot_loan`.

use crate::model::{as_addr_of, unwrap_parens, Checker, FnState, TypeShape};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt, Severity, Span};
use hy_syntax::{
    AssignOp, Block, Decl, Expr, FnDecl, ForKind, InterpPart, LocalDecl, LocalKind, MatchPattern,
    Place, PlaceKind, SelectArmKind, Stmt, TypeExpr,
};

fn unwrap_parens_and_moves(mut e: &Expr) -> &Expr {
    while let Expr::Paren { inner, .. } | Expr::Move { operand: inner, .. } = e {
        e = inner;
    }
    e
}

fn place_root_ident(mut p: &Place) -> Option<&str> {
    loop {
        match &p.kind {
            PlaceKind::Ident(n) => return Some(n.as_str()),
            PlaceKind::Field { base, .. }
            | PlaceKind::Index { base, .. }
            | PlaceKind::AddrOf { base } => p = base,
        }
    }
}

fn place_to_expr(p: &Place) -> Expr {
    match &p.kind {
        PlaceKind::Ident(name) => Expr::Ident {
            name: name.clone(),
            span: p.span,
        },
        PlaceKind::Index { base, index } => Expr::Index {
            base: Box::new(place_to_expr(base)),
            index: index.clone(),
            span: p.span,
        },
        PlaceKind::Field { base, name } => Expr::Field {
            base: Box::new(place_to_expr(base)),
            name: name.clone(),
            span: p.span,
        },
        PlaceKind::AddrOf { base } => Expr::AddrOf {
            operand: Box::new(place_to_expr(base)),
            span: p.span,
        },
    }
}

fn is_slot_loan_expr_ext(ck: &Checker<'_>, st: &FnState, e: &Expr) -> bool {
    walk::addr_of_index_base_path(e).is_some_and(|p| {
        ck.lookup_binding(st, p.split('.').next().unwrap_or(&p))
            .is_some_and(|b| {
                matches!(
                    b.shape,
                    TypeShape::Map(..)
                        | TypeShape::Slice(_)
                        | TypeShape::Array(_)
                        | TypeShape::Struct(_)
                        | TypeShape::Pointer(_)
                )
            })
    })
}

fn is_slot_loan_or_alias_expr_ext(
    ck: &Checker<'_>,
    st: &FnState,
    e: &Expr,
    allow_addr_root: bool,
) -> bool {
    if allow_addr_root && is_slot_loan_expr_ext(ck, st, e) {
        return true;
    }
    matches!(
        unwrap_parens_and_moves(e),
        Expr::Ident { name, .. } if ck.lookup_binding(st, name).is_some_and(|b| b.slot_loan)
    )
}

impl<'a> Checker<'a> {
    pub(crate) fn check_addr_loan_targets_and_loops(&mut self) {
        let fns: Vec<&FnDecl> = self.file.fns().collect();
        for f in fns {
            self.check_fn_addr_loan_targets(f);
        }
    }

    fn check_fn_addr_loan_targets(&mut self, f: &FnDecl) {
        let Some(body) = &f.body else {
            return;
        };
        let mut st = FnState::default();
        for tp in &f.type_params {
            st.type_params.insert(tp.name.clone());
        }
        if let Some(r) = &f.receiver {
            let base_shape = self.shape_of_type(&r.ty);
            let (shape, ty) = if r.is_pointer {
                (
                    TypeShape::Pointer(Box::new(base_shape)),
                    TypeExpr::Pointer {
                        to: Box::new(r.ty.clone()),
                        span: r.ty.span(),
                    },
                )
            } else {
                (base_shape, r.ty.clone())
            };
            self.define_binding(&mut st, &r.name, shape, Some(ty), false, false, false);
        }
        for p in &f.params {
            let shape = self.shape_of_type(&p.ty);
            self.define_binding(&mut st, &p.name, shape, Some(p.ty.clone()), false, false, false);
        }
        for r in &f.results {
            if let Some(name) = &r.name {
                if name != "_" {
                    let shape = self.shape_of_type(&r.ty);
                    self.define_binding(
                        &mut st,
                        name,
                        shape,
                        Some(r.ty.clone()),
                        false,
                        false,
                        false,
                    );
                }
            }
        }
        st.result_types = f.results.iter().map(|r| r.ty.clone()).collect();
        for s in &body.stmts {
            self.walk_stmt_addr_loan_ext(&mut st, s);
        }
    }

    fn walk_block_addr_loan_ext(&mut self, st: &mut FnState, b: &Block) {
        st.scope_push();
        for s in &b.stmts {
            self.walk_stmt_addr_loan_ext(st, s);
        }
        st.scope_pop();
    }

    fn bind_local_addr_loan_ext(&self, st: &mut FnState, l: &LocalDecl) {
        let immutable = matches!(l.kind, LocalKind::Const);
        let is_let = matches!(l.kind, LocalKind::Let);
        let allow_addr_root =
            matches!(l.kind, LocalKind::Define | LocalKind::Let) && l.ty.is_none();
        if l.values.is_empty() {
            let shape = l
                .ty
                .as_ref()
                .map(|t| self.shape_of_type(t))
                .unwrap_or(TypeShape::Unknown);
            let ty = l.ty.clone();
            for name in &l.names {
                self.define_binding(st, name, shape.clone(), ty.clone(), immutable, false, is_let);
            }
        } else if l.names.len() == l.values.len() {
            let flags: Vec<bool> = l
                .values
                .iter()
                .map(|val| is_slot_loan_or_alias_expr_ext(self, st, val, allow_addr_root))
                .collect();
            for ((name, val), is_loan) in l.names.iter().zip(l.values.iter()).zip(flags) {
                let inferred = self.shape_of_expr(st, val);
                let shape = match (&l.ty, inferred) {
                    (Some(t), TypeShape::Unknown) => self.shape_of_type(t),
                    (_, s) => s,
                };
                let ty = l.ty.clone().or_else(|| self.ty_of_expr(st, val));
                self.define_binding(st, name, shape, ty, immutable, false, is_let);
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan = is_loan;
                }
            }
        } else if l.values.len() == 1 && l.names.len() > 1 {
            let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
            for (name, shape) in l.names.iter().zip(shapes) {
                self.define_binding(st, name, shape, None, immutable, false, is_let);
            }
        }
    }

    fn mark_assign_addr_loan_ext(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[Expr],
    ) {
        if op != AssignOp::Assign || targets.len() != values.len() {
            return;
        }
        let allow_addr_root = targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
        let flags: Vec<bool> = values
            .iter()
            .map(|v| is_slot_loan_or_alias_expr_ext(self, st, v, allow_addr_root))
            .collect();
        for (t, is_loan) in targets.iter().zip(flags) {
            if let PlaceKind::Ident(name) = &t.kind {
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan |= is_loan;
                }
            }
        }
    }

    fn check_rebind_addr_loan_ext(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                ..
            } => {
                if targets.len() == values.len() {
                    let allow_addr_root =
                        targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
                    for (t, v) in targets.iter().zip(values.iter()) {
                        let PlaceKind::Ident(name) = &t.kind else {
                            continue;
                        };
                        let Some(b) = self.lookup_binding(st, name) else {
                            continue;
                        };
                        if !b.slot_loan {
                            continue;
                        }
                        if allow_addr_root && walk::addr_of_index_base_path(v).is_some() {
                            continue;
                        }
                        if (!allow_addr_root && walk::addr_of_index_base_path(v).is_some())
                            || self.is_general_pointer_value_ext(st, v)
                        {
                            self.emit_slot_loan_rebind_diag_ext(name, &v.render(), t.span);
                        }
                    }
                } else if values.len() == 1 && targets.len() > 1 {
                    let shapes = self.multi_value_shapes(st, &values[0], targets.len());
                    for (t, shape) in targets.iter().zip(shapes.iter()) {
                        let PlaceKind::Ident(name) = &t.kind else {
                            continue;
                        };
                        let Some(b) = self.lookup_binding(st, name) else {
                            continue;
                        };
                        if !b.slot_loan || !matches!(shape, TypeShape::Pointer(_)) {
                            continue;
                        }
                        self.emit_slot_loan_rebind_diag_ext(name, &values[0].render(), t.span);
                    }
                }
            }
            Stmt::Local(l) if l.kind == LocalKind::Define && l.ty.is_none() => {
                if l.names.len() == l.values.len() {
                    for (name, v) in l.names.iter().zip(l.values.iter()) {
                        let Some(b) = Self::current_scope_binding_ext(st, name) else {
                            continue;
                        };
                        if !b.slot_loan
                            || walk::addr_of_index_base_path(v).is_some()
                            || !self.is_general_pointer_value_ext(st, v)
                        {
                            continue;
                        }
                        self.emit_slot_loan_rebind_diag_ext(name, &v.render(), l.span);
                    }
                } else if l.values.len() == 1 && l.names.len() > 1 {
                    let shapes = self.multi_value_shapes(st, &l.values[0], l.names.len());
                    for (name, shape) in l.names.iter().zip(shapes.iter()) {
                        let Some(b) = Self::current_scope_binding_ext(st, name) else {
                            continue;
                        };
                        if !b.slot_loan || !matches!(shape, TypeShape::Pointer(_)) {
                            continue;
                        }
                        self.emit_slot_loan_rebind_diag_ext(name, &l.values[0].render(), l.span);
                    }
                }
            }
            _ => {}
        }
    }

    fn current_scope_binding_ext<'s>(
        st: &'s FnState,
        name: &str,
    ) -> Option<&'s crate::model::Binding> {
        match st.scopes.last() {
            Some(top) => top.get(name),
            None => st.env.get(name),
        }
    }

    fn is_general_pointer_value_ext(&self, st: &FnState, v: &Expr) -> bool {
        let u = unwrap_parens_and_moves(v);
        if matches!(
            u,
            Expr::Lit {
                kind: hy_syntax::LitKind::Nil,
                ..
            }
        ) {
            return true;
        }
        if as_addr_of(u).is_some() {
            return walk::addr_of_index_base_path(v).is_none();
        }
        if let Expr::Ident { name, .. } = u {
            if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                return false;
            }
        }
        matches!(self.shape_of_expr(st, u), TypeShape::Pointer(_))
    }

    fn walk_stmt_addr_loan_ext(&mut self, st: &mut FnState, stmt: &Stmt) {
        let mut uses = Vec::new();
        walk::own_uses_with_results(stmt, &st.named_results, &mut uses);
        for u in uses.iter().filter(|u| u.via_addr || u.via_move) {
            if self.lookup_binding(st, &u.name).is_some_and(|b| b.slot_loan) {
                self.emit_addr_of_loan_diag_ext(&u.name, u.span);
            }
        }
        self.check_rebind_addr_loan_ext(st, stmt);

        match stmt {
            Stmt::Local(l) => {
                if let Some(ty) = &l.ty {
                    self.walk_type_addr_loan_ext(st, ty);
                }
                let allow_slot_loan = matches!(l.kind, LocalKind::Define | LocalKind::Let)
                    && l.ty.is_none()
                    && l.names.len() == l.values.len();
                for v in &l.values {
                    let allow_root = allow_slot_loan && walk::addr_of_index_base_path(v).is_some();
                    self.walk_expr_addr_loan_ext(st, v, allow_root);
                }
                self.bind_local_addr_loan_ext(st, l);
            }
            Stmt::Assign {
                targets,
                op,
                values,
                ..
            } => {
                for t in targets {
                    self.walk_place_addr_loan_ext(st, t);
                }
                let allow_slot_loan = *op == AssignOp::Assign
                    && targets.len() == values.len()
                    && targets.iter().all(|t| matches!(&t.kind, PlaceKind::Ident(_)));
                for v in values {
                    let allow_root = allow_slot_loan && walk::addr_of_index_base_path(v).is_some();
                    self.walk_expr_addr_loan_ext(st, v, allow_root);
                }
                self.mark_assign_addr_loan_ext(st, targets, *op, values);
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.walk_expr_addr_loan_ext(st, expr, false);
            }
            Stmt::Send {
                chan,
                value,
                is_move,
                span,
            } => {
                if *is_move {
                    if let Some(root) = value.root_ident() {
                        if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag_ext(root, *span);
                        }
                    }
                }
                self.walk_expr_addr_loan_ext(st, chan, false);
                self.walk_expr_addr_loan_ext(st, value, false);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                st.scope_push();
                if let Some(i) = init {
                    self.walk_stmt_addr_loan_ext(st, i);
                }
                self.walk_expr_addr_loan_ext(st, cond, false);
                self.walk_block_addr_loan_ext(st, then);
                if let Some(other) = otherwise {
                    st.scope_push();
                    self.walk_stmt_addr_loan_ext(st, other);
                    st.scope_pop();
                }
                st.scope_pop();
            }
            Stmt::For { kind, body, .. } => {
                st.scope_push();
                match kind {
                    ForKind::While { cond } => {
                        self.walk_expr_addr_loan_ext(st, cond, false);
                    }
                    ForKind::Range { var, lo, hi, .. } => {
                        self.walk_expr_addr_loan_ext(st, lo, false);
                        self.walk_expr_addr_loan_ext(st, hi, false);
                        self.define_binding(
                            st,
                            var,
                            TypeShape::Primitive,
                            None,
                            false,
                            false,
                            false,
                        );
                    }
                    ForKind::Iterate { vars, expr } => {
                        self.walk_expr_addr_loan_ext(st, expr, false);
                        let shapes = self.iterate_var_shapes(st, expr, vars.len());
                        for (name, shape) in vars.iter().zip(shapes) {
                            self.define_binding(st, name, shape, None, false, false, false);
                        }
                    }
                    ForKind::Infinite => {}
                }
                self.walk_block_addr_loan_ext(st, body);
                // Second pass (loop backedge replay): any outer binding marked
                // `slot_loan = true` inside `body` on iteration 1 is live on
                // iteration 2 when re-evaluating `While { cond }` and `body`.
                if let ForKind::While { cond } = kind {
                    self.walk_expr_addr_loan_ext(st, cond, false);
                }
                self.walk_block_addr_loan_ext(st, body);
                st.scope_pop();
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    let bare = unwrap_parens_and_moves(v);
                    if let Expr::Ident { name, .. } = bare {
                        if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag_ext(name, v.span());
                        }
                    }
                    let e = as_addr_of(unwrap_parens(v))
                        .filter(|_| self.map_slot_borrow_span_ext(st, v).is_some())
                        .unwrap_or(v);
                    self.walk_expr_addr_loan_ext(st, e, false);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.walk_expr_addr_loan_ext(st, callee, false);
                for a in args {
                    self.walk_expr_addr_loan_ext(st, &a.value, false);
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.walk_expr_addr_loan_ext(st, subject, false);
                for arm in arms {
                    st.scope_push();
                    if let MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        self.bind_match_arm(st, b, subject, false);
                    }
                    if let Some(g) = &arm.guard {
                        self.walk_expr_addr_loan_ext(st, g, false);
                    }
                    self.walk_block_addr_loan_ext(st, &arm.body);
                    st.scope_pop();
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    st.scope_push();
                    match &arm.kind {
                        SelectArmKind::Send {
                            chan,
                            value,
                            is_move,
                        } => {
                            if *is_move {
                                if let Some(root) = value.root_ident() {
                                    if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                                        self.emit_addr_of_loan_diag_ext(root, value.span());
                                    }
                                }
                            }
                            self.walk_expr_addr_loan_ext(st, chan, false);
                            self.walk_expr_addr_loan_ext(st, value, false);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            self.walk_expr_addr_loan_ext(st, chan, false);
                            let elem = match self.shape_of_expr(st, chan) {
                                TypeShape::Chan(inner) => *inner,
                                _ => TypeShape::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                if n != "_" {
                                    let shape = if i == 0 {
                                        elem.clone()
                                    } else {
                                        TypeShape::Primitive
                                    };
                                    self.define_binding(st, n, shape, None, false, false, false);
                                }
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                    self.walk_block_addr_loan_ext(st, &arm.body);
                    st.scope_pop();
                }
            }
            Stmt::Block { block: body, .. } | Stmt::Managed { body, .. } => {
                self.walk_block_addr_loan_ext(st, body);
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn walk_place_addr_loan_ext(&mut self, st: &mut FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.walk_place_addr_loan_ext(st, base);
                self.walk_expr_addr_loan_ext(st, index, false);
            }
            PlaceKind::Field { base, .. } => {
                self.walk_place_addr_loan_ext(st, base);
            }
            PlaceKind::AddrOf { base } => {
                let base_expr = place_to_expr(base);
                if self.is_map_element_or_value_projection_ext(st, &base_expr) {
                    self.emit_map_slot_borrow_diag_ext(place.span);
                } else if let Some(root) = place_root_ident(base) {
                    if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag_ext(root, place.span);
                    }
                }
                self.walk_place_addr_loan_ext(st, base);
            }
        }
    }

    fn walk_expr_addr_loan_ext(
        &mut self,
        st: &mut FnState,
        expr: &Expr,
        allow_slot_loan_root: bool,
    ) {
        if !allow_slot_loan_root {
            if let Some(span) = self.map_slot_borrow_span_ext(st, expr) {
                self.emit_map_slot_borrow_diag_ext(span);
            }
        }
        if let Expr::AddrOf { operand, span }
        | Expr::Unary {
            op: hy_syntax::UnOp::Addr,
            operand,
            span,
        } = expr
        {
            match unwrap_parens_and_moves(operand) {
                Expr::Ident { name, span: id_span } => {
                    if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag_ext(name, *id_span);
                    }
                }
                Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
                    if let Some(root) = unwrap_parens_and_moves(operand).root_ident() {
                        if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag_ext(root, *span);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Expr::Move { operand, span } = expr {
            match unwrap_parens_and_moves(operand) {
                Expr::Ident { name, .. } => {
                    if self.lookup_binding(st, name).is_some_and(|b| b.slot_loan) {
                        self.emit_addr_of_loan_diag_ext(name, *span);
                    }
                }
                Expr::Field { .. } | Expr::Index { .. } | Expr::Slice { .. } => {
                    if let Some(root) = unwrap_parens_and_moves(operand).root_ident() {
                        if self.lookup_binding(st, root).is_some_and(|b| b.slot_loan) {
                            self.emit_addr_of_loan_diag_ext(root, *span);
                        }
                    }
                }
                _ => {}
            }
        }
        match expr {
            Expr::Binary { lhs, rhs, .. } => {
                self.walk_expr_addr_loan_ext(st, lhs, false);
                self.walk_expr_addr_loan_ext(st, rhs, false);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                self.walk_expr_addr_loan_ext(st, operand, false);
            }
            Expr::Index { base, index, .. } => {
                self.walk_expr_addr_loan_ext(st, base, false);
                self.walk_expr_addr_loan_ext(st, index, false);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.walk_expr_addr_loan_ext(st, base, false);
                if let Some(lo) = lo {
                    self.walk_expr_addr_loan_ext(st, lo, false);
                }
                if let Some(hi) = hi {
                    self.walk_expr_addr_loan_ext(st, hi, false);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.walk_expr_addr_loan_ext(st, callee, false);
                for a in args {
                    self.walk_expr_addr_loan_ext(st, &a.value, false);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.walk_type_addr_loan_ext(st, t);
                }
                for f in fields {
                    self.walk_expr_addr_loan_ext(st, &f.value, false);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.walk_type_addr_loan_ext(st, t);
                }
                for i in items {
                    self.walk_expr_addr_loan_ext(st, i, false);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.walk_type_addr_loan_ext(st, key);
                self.walk_type_addr_loan_ext(st, value);
                for (k, v) in entries {
                    self.walk_expr_addr_loan_ext(st, k, false);
                    self.walk_expr_addr_loan_ext(st, v, false);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.walk_expr_addr_loan_ext(st, e, false);
                    }
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.walk_type_addr_loan_ext(st, &p.ty);
                }
                for r in results {
                    self.walk_type_addr_loan_ext(st, &r.ty);
                }
                let mut cst = st.clone();
                cst.scope_push();
                for p in params {
                    let shape = self.shape_of_type(&p.ty);
                    self.define_binding(&mut cst, &p.name, shape, Some(p.ty.clone()), false, false, false);
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
                                false,
                            );
                        }
                    }
                }
                cst.result_types = results.iter().map(|r| r.ty.clone()).collect();
                for s in &body.stmts {
                    self.walk_stmt_addr_loan_ext(&mut cst, s);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn walk_type_addr_loan_ext(&mut self, st: &mut FnState, ty: &TypeExpr) {
        match ty {
            TypeExpr::Array { len, elem, .. } => {
                if let Some(len_expr) = len {
                    self.walk_expr_addr_loan_ext(st, len_expr, false);
                }
                self.walk_type_addr_loan_ext(st, elem);
            }
            TypeExpr::Pointer { to: inner, .. }
            | TypeExpr::Slice { elem: inner, .. }
            | TypeExpr::Chan { elem: inner, .. }
            | TypeExpr::Option { elem: inner, .. } => {
                self.walk_type_addr_loan_ext(st, inner);
            }
            TypeExpr::Map { key, value, .. } => {
                self.walk_type_addr_loan_ext(st, key);
                self.walk_type_addr_loan_ext(st, value);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for t in params.iter().map(|p| &p.ty).chain(results.iter()) {
                    self.walk_type_addr_loan_ext(st, t);
                }
            }
            TypeExpr::Named { .. } => {}
        }
    }

    fn map_slot_borrow_span_ext(&self, st: &FnState, e: &Expr) -> Option<Span> {
        let (operand, span) = match unwrap_parens_and_moves(e) {
            Expr::AddrOf { operand, span } => (operand.as_ref(), *span),
            Expr::Unary {
                op: hy_syntax::UnOp::Addr,
                operand,
                span,
            } => (operand.as_ref(), *span),
            _ => return None,
        };
        if self.is_map_element_or_value_projection_ext(st, operand) {
            Some(span)
        } else {
            None
        }
    }

    fn is_map_element_or_value_projection_ext(&self, st: &FnState, e: &Expr) -> bool {
        match unwrap_parens_and_moves(e) {
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(..) => true,
                TypeShape::Array(_) => self.is_map_element_or_value_projection_ext(st, base),
                _ => false,
            },
            Expr::Field { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Struct(_) => self.is_map_element_or_value_projection_ext(st, base),
                _ => false,
            },
            _ => false,
        }
    }

    fn emit_slot_loan_rebind_diag_ext(&mut self, name: &str, rhs_rendered: &str, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.span == span)
        {
            return;
        }
        let msg = format!(
            "`{} = {}` re-binds the map-slot loan `{}` to a general pointer: a loan is not a pointer (T24) and cannot hold one",
            name, rhs_rendered, name
        );
        let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), span, msg).with_notes(vec![
            "an O8 loan `&m[k]` is a checker fact, not a runtime pointer: it lowers to a copy of the slot plus a store-back at its last use, so the binding holds a value, not an address".to_string(),
            "re-binding it to a general pointer emits a Go assignment that cannot build, or makes the store-back write the new pointee into the map slot".to_string(),
        ]);
        d.fixits = vec![FixIt {
            description: format!(
                "bind the loan to its own name (`n := &m[k]`) and keep `{}` for the pointer",
                name
            ),
            replacement: None,
        }];
        self.diags.push(d);
    }

    fn emit_addr_of_loan_diag_ext(&mut self, name: &str, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.span == span)
        {
            return;
        }
        let msg = format!(
            "`&{}` takes the address of a loan: a map-slot loan is not an addressable value (T24)",
            name
        );
        let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), span, msg).with_notes(vec![
            format!(
                "`{}` is already the only general-value `&` Hybrid has (an O8 map-slot loan); a pointer to it would be a general pointer, which Tier 0 does not have",
                name
            ),
        ]);
        d.fixits = vec![FixIt {
            description: format!(
                "use `{}` directly, or take a fresh loan of the slot (`&m[k]`) where it is needed",
                name
            ),
            replacement: None,
        }];
        self.diags.push(d);
    }

    fn emit_map_slot_borrow_diag_ext(&mut self, span: Span) {
        if self
            .diags
            .iter()
            .any(|d| d.severity == Severity::Error && d.span == span)
        {
            return;
        }
        let mut d = Diagnostic::error(
            "HYB-E3104",
            self.path.clone(),
            span,
            "`&m[k]` here has no lowering: a map-slot borrow is only a loan where it is the whole value of a binding (`n := &m[k]`), and Go cannot take the address of a map element".to_string(),
        )
        .with_notes(vec![
            "the emitter lowers a slot loan to a copy of the slot plus a store-back at the binding's last use; in an argument or operand position there is no binding to store back to, so `&m[k]` is emitted as written".to_string(),
            "the emitted program does not build: `invalid operation: cannot take address of m[k] (map index expression)`".to_string(),
        ]);
        d.fixits = vec![FixIt {
            description: "bind the loan to its own name first (`n := &m[k]`) and pass `n`, or pass the element value and store it back after the call".to_string(),
            replacement: None,
        }];
        self.diags.push(d);
    }
}
