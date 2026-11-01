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
use hy_syntax::{Block, Decl, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt};

/// Whether the expression sits in a composite element slot, where the
/// emitter inherits the head from the enclosing literal's type.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Slot {
    Free,
    Element,
}

impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_literals(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => self.elided_expr(&c.value, Slot::Free),
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
                    self.elided_expr(v, Slot::Free);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.elided_place(t);
                }
                for v in values {
                    self.elided_expr(v, Slot::Free);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.elided_expr(expr, Slot::Free)
            }
            Stmt::Send { chan, value, .. } => {
                self.elided_expr(chan, Slot::Free);
                self.elided_expr(value, Slot::Free);
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
                self.elided_expr(cond, Slot::Free);
                self.elided_block(then);
                if let Some(o) = otherwise {
                    self.elided_stmt(o);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.elided_expr(cond, Slot::Free),
                    ForKind::Range { lo, hi, .. } => {
                        self.elided_expr(lo, Slot::Free);
                        self.elided_expr(hi, Slot::Free);
                    }
                    ForKind::Iterate { expr, .. } => self.elided_expr(expr, Slot::Free),
                }
                self.elided_block(body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.elided_expr(v, Slot::Free);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.elided_expr(callee, Slot::Free);
                for a in args {
                    self.elided_expr(&a.value, Slot::Free);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                self.elided_block(body)
            }
            Stmt::Match { subject, arms, .. } => {
                self.elided_expr(subject, Slot::Free);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.elided_expr(g, Slot::Free);
                    }
                    self.elided_block(&a.body);
                }
            }
            Stmt::Select { arms, .. } => {
                for a in arms {
                    match &a.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.elided_expr(chan, Slot::Free);
                            self.elided_expr(value, Slot::Free);
                        }
                        SelectArmKind::Recv { chan, .. } => self.elided_expr(chan, Slot::Free),
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
                self.elided_expr(index, Slot::Free);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => self.elided_place(base),
        }
    }

    fn elided_expr(&mut self, e: &Expr, slot: Slot) {
        match e {
            // `emit_expr_exp` peels parentheses before filling the head.
            Expr::Paren { inner, .. } => self.elided_expr(inner, slot),
            Expr::StructLit { ty, fields, span } => {
                if ty.is_none() && slot == Slot::Free {
                    self.push_elided_diag(*span);
                }
                // Field values are emitted without an expected type.
                for f in fields {
                    self.elided_expr(&f.value, Slot::Free);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                // A typed slice/array literal hands its items the element
                // type; an untyped `[...]` does so only when it itself sits
                // in an element slot (`emit_expr_exp` recursion). Otherwise
                // its items are emitted with no expected type.
                let item_slot = if elem.is_some() || slot == Slot::Element {
                    Slot::Element
                } else {
                    Slot::Free
                };
                for i in items {
                    self.elided_expr(i, item_slot);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.elided_expr(k, Slot::Element);
                    self.elided_expr(v, Slot::Element);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.elided_expr(x, Slot::Free);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.elided_expr(lhs, Slot::Free);
                self.elided_expr(rhs, Slot::Free);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.elided_expr(operand, Slot::Free),
            Expr::Receive { chan, .. } => self.elided_expr(chan, Slot::Free),
            Expr::Index { base, index, .. } => {
                self.elided_expr(base, Slot::Free);
                self.elided_expr(index, Slot::Free);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.elided_expr(base, Slot::Free);
                if let Some(l) = lo {
                    self.elided_expr(l, Slot::Free);
                }
                if let Some(h) = hi {
                    self.elided_expr(h, Slot::Free);
                }
            }
            Expr::Field { base, .. } => self.elided_expr(base, Slot::Free),
            Expr::Call { callee, args, .. } => {
                self.elided_expr(callee, Slot::Free);
                for a in args {
                    self.elided_expr(&a.value, Slot::Free);
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
