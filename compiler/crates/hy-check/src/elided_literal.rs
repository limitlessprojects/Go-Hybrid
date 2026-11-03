//! elided_literal.rs — HYB-E1002: a type-elided struct literal `{...}` the
//! emitter has no type for (GUARANTEES Row 13, accepted ⇒ buildable).
//!
//! hy-emit fills an elided struct head only from an EXPECTED type, and it
//! has one only in composite element slots: the items of a typed slice or
//! array literal (`[]point{{x: 1}}`), the keys and values of a map literal
//! (`map[string]point{"a": {x: 1}}`), and — recursively — the items of an
//! untyped `[...]` sitting in such a slot (`[][]point{[{x: 1}]}`); see
//! `emit_expr_exp`. Everywhere else it refuses the literal with
//! `emitter (MVP-0): not yet supported: type-elided struct literal — spell
//! the type`. Before this pass `hyb check` accepted every one of those
//! programs with 0 diagnostics — `v := {x: 1}`, `show({x: 1})`,
//! `return {x: 1}`, `var v point = {x: 1}`, `v = {x: 1}`, `ch <- {x: 1}`,
//! `append(xs, {x: 1})`, `wrap{inner: {x: 1}}`, `[point{x: 1}, {x: 2}]` —
//! and `hyb build` then failed.
//!
//! The walk is syntactic and mirrors the emitter's slot rules exactly
//! (parentheses are transparent, as in `emit_expr_exp`), so a literal the
//! emitter can lower is never rejected here.

use crate::model::Checker;
use hy_diag::Diagnostic;
use hy_syntax::{Block, Decl, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt, TypeExpr};

/// Whether the expression sits in a composite element slot, where the
/// emitter inherits the head from the enclosing literal's type.
impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_literals(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => self.elided_expr(&c.value, None),
                Decl::Fn(f) => {
                    if let Some(b) = &f.body {
                        self.elided_block(b);
                    }
                }
                _ => {}
            }
        }
    }

    fn elided_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.elided_stmt(s);
        }
    }

    fn elided_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.elided_expr(v, None);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.elided_place(t);
                }
                for v in values {
                    self.elided_expr(v, None);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.elided_expr(expr, None)
            }
            Stmt::Send { chan, value, .. } => {
                self.elided_expr(chan, None);
                self.elided_expr(value, None);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.elided_stmt(i);
                }
                self.elided_expr(cond, None);
                self.elided_block(then);
                if let Some(o) = otherwise {
                    self.elided_stmt(o);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.elided_expr(cond, None),
                    ForKind::Range { lo, hi, .. } => {
                        self.elided_expr(lo, None);
                        self.elided_expr(hi, None);
                    }
                    ForKind::Iterate { expr, .. } => self.elided_expr(expr, None),
                }
                self.elided_block(body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.elided_expr(v, None);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.elided_expr(callee, None);
                for a in args {
                    self.elided_expr(&a.value, None);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                self.elided_block(body)
            }
            Stmt::Match { subject, arms, .. } => {
                self.elided_expr(subject, None);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.elided_expr(g, None);
                    }
                    self.elided_block(&a.body);
                }
            }
            Stmt::Select { arms, .. } => {
                for a in arms {
                    match &a.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.elided_expr(chan, None);
                            self.elided_expr(value, None);
                        }
                        SelectArmKind::Recv { chan, .. } => self.elided_expr(chan, None),
                        SelectArmKind::Default => {}
                    }
                    self.elided_block(&a.body);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn elided_place(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.elided_place(base);
                self.elided_expr(index, None);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => self.elided_place(base),
        }
    }

    fn is_elidable_struct_slot(&self, exp: Option<&TypeExpr>, is_empty: bool) -> bool {
        let Some(t) = exp else { return false };
        let mut r = self.resolve_type_alias(t);
        if let TypeExpr::Pointer { to, .. } = r {
            r = self.resolve_type_alias(&to);
        }
        if is_empty && matches!(r, TypeExpr::Map { .. } | TypeExpr::Slice { .. } | TypeExpr::Array { .. }) {
            return true;
        }
        let TypeExpr::Named { name, .. } = &r else { return false };
        if name.starts_with("struct{") || name.starts_with("struct ") {
            return true;
        }
        let head = name.split('[').next().unwrap_or(name.as_str());
        self.structs.contains_key(head) || head.contains('.')
    }

    fn elided_expr(&mut self, e: &Expr, expected: Option<&TypeExpr>) {
        match e {
            // `emit_expr_exp` peels parentheses before filling the head.
            Expr::Paren { inner, .. } => self.elided_expr(inner, expected),
            Expr::StructLit { ty, fields, span } => {
                if ty.is_none() && !self.is_elidable_struct_slot(expected, fields.is_empty()) {
                    self.push_elided_diag(*span);
                }
                // Field values are emitted without an expected type.
                for f in fields {
                    self.elided_expr(&f.value, None);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                let resolved_exp = expected.map(|t| self.resolve_type_alias(t));
                let item_exp = if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    Some(inner_elem)
                } else if let Some(t) = elem.as_deref() {
                    Some(t)
                } else {
                    match &resolved_exp {
                        Some(TypeExpr::Slice { elem: inner, .. } | TypeExpr::Array { elem: inner, .. }) => {
                            Some(inner.as_ref())
                        }
                        _ => None,
                    }
                };
                for i in items {
                    self.elided_expr(i, item_exp);
                }
            }
            Expr::MapLit { key, value, entries, .. } => {
                for (k, v) in entries {
                    self.elided_expr(k, Some(key));
                    self.elided_expr(v, Some(value));
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.elided_expr(x, None);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.elided_expr(lhs, None);
                self.elided_expr(rhs, None);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.elided_expr(operand, None),
            Expr::Receive { chan, .. } => self.elided_expr(chan, None),
            Expr::Index { base, index, .. } => {
                self.elided_expr(base, None);
                self.elided_expr(index, None);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.elided_expr(base, None);
                if let Some(l) = lo {
                    self.elided_expr(l, None);
                }
                if let Some(h) = hi {
                    self.elided_expr(h, None);
                }
            }
            Expr::Field { base, .. } => self.elided_expr(base, None),
            Expr::Call { callee, args, .. } => {
                self.elided_expr(callee, None);
                for a in args {
                    self.elided_expr(&a.value, None);
                }
            }
            Expr::FnLit { body, .. } => self.elided_block(body),
        }
    }

    fn push_elided_diag(&mut self, span: hy_diag::Span) {
        self.diags.push(
            Diagnostic::error(
                "HYB-E1002",
                self.path.clone(),
                span,
                "type-elided struct literal `{...}` has no type to inherit here — `hyb build` refuses it (HYB-E1002)".to_string(),
            )
            .with_notes(vec![
                "why: hy-emit fills an elided struct head only from an enclosing typed slice, array, or map literal; anywhere else the accepted program fails `hyb build` with `type-elided struct literal — spell the type` (GUARANTEES Row 13)".to_string(),
                "fix: write the struct type before the braces (`point{x: 1}`), or put the literal in a typed composite (`[]point{{x: 1}}`)".to_string(),
            ]),
        );
    }
}
