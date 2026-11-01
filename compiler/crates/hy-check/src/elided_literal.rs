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
use crate::resolve::is_predeclared_type_name;
use hy_diag::Diagnostic;
use hy_syntax::{
    Block, Decl, Expr, FieldInit, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr,
};
use std::collections::HashMap;

/// Whether the expression sits in a composite element slot, where the
/// emitter inherits the head from the enclosing literal's type.
#[derive(Clone, Copy)]
enum Slot<'t> {
    Free,
    Element(&'t TypeExpr),
}

impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_literals(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => {
                    if let Some(ty) = &c.ty {
                        self.elided_type(ty);
                    }
                    self.elided_expr(&c.value, Slot::Free);
                }
                Decl::Struct(s) => s.fields.iter().for_each(|f| self.elided_type(&f.ty)),
                Decl::TypeAlias(a) => self.elided_type(&a.ty),
                Decl::Enum(e) => e.variants.iter().filter_map(|v| v.payload.as_ref()).for_each(|p| self.elided_type(p)),
                Decl::Fn(f) => {
                    if let Some(r) = &f.receiver {
                        self.elided_type(&r.ty);
                    }
                    f.params.iter().for_each(|p| self.elided_type(&p.ty));
                    f.results.iter().for_each(|r| self.elided_type(&r.ty));
                    if let Some(b) = &f.body {
                        self.elided_block(b);
                    }
                }
                Decl::Import { .. } => {}
            }
        }
    }

    fn elided_type(&mut self, ty: &TypeExpr) {
        match ty {
            TypeExpr::Named { .. } => {}
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. }
            | TypeExpr::Option { elem: to, .. } => self.elided_type(to),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.elided_expr(l, Slot::Free);
                }
                self.elided_type(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.elided_type(key);
                self.elided_type(value);
            }
            TypeExpr::Fn { params, results, .. } => {
                params.iter().for_each(|p| self.elided_type(&p.ty));
                results.iter().for_each(|r| self.elided_type(r));
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
                if let Some(ty) = &l.ty {
                    self.elided_type(ty);
                }
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

    fn is_valid_elided_struct_head_name(&self, full_name: &str, fields_empty: bool) -> bool {
        let base = Self::base_named_type_str(full_name);
        if base.is_empty()
            || (!self.structs.contains_key(base) && is_predeclared_type_name(base))
            || self.enums.contains_key(base)
        {
            return false;
        }
        if base == "set" {
            return fields_empty;
        }
        if base.contains('.') {
            return !matches!(
                base,
                "http.handler"
                    | "http.response_writer"
                    | "io.reader"
                    | "io.writer"
                    | "io.closer"
                    | "io.read_closer"
                    | "context.context"
                    | "fmt.stringer"
            );
        }
        self.structs.contains_key(base)
    }

    fn check_elided_struct_fields(&mut self, full_name: &str, fields: &[FieldInit]) {
        let sname = Self::base_named_type_str(full_name).to_string();
        let Some(declared) = self.structs.get(&sname).cloned() else {
            return;
        };
        let mut seen: HashMap<String, usize> = HashMap::new();
        for f in fields {
            if f.name.is_empty() {
                continue;
            }
            let n = seen.entry(f.name.clone()).or_insert(0);
            *n += 1;
            if *n == 2 {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        f.span,
                        format!(
                            "duplicate field `{}` in `{}` literal — Go refuses `duplicate field name {} in struct literal` (HYB-E1002)",
                            f.name, full_name, f.name
                        ),
                    )
                    .with_notes(vec![
                        "each field may be set at most once in one struct literal".to_string(),
                    ]),
                );
            }
        }
        for f in fields {
            if !f.name.is_empty() && !declared.iter().any(|(n, _)| n == &f.name) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        f.span,
                        format!(
                            "unknown field `{}` in `{}` literal — Go refuses `unknown field {} in struct literal of type {}` (HYB-E1002)",
                            f.name, full_name, f.name, full_name
                        ),
                    )
                    .with_notes(vec![
                        "a struct literal may only name fields the struct declares".to_string(),
                    ]),
                );
            }
        }
    }

    fn check_elided_struct_head(&mut self, slot: Slot<'_>, fields: &[FieldInit], span: hy_diag::Span) {
        let Slot::Element(exp) = slot else {
            self.push_elided_diag(span);
            return;
        };
        if let TypeExpr::Pointer { to, .. } = exp {
            if let TypeExpr::Named { name, .. } = self.resolve_type_alias(to) {
                if self.is_valid_elided_struct_head_name(&name, fields.is_empty()) {
                    self.check_elided_struct_fields(&name, fields);
                    return;
                }
            }
            self.push_elided_diag(span);
            return;
        }
        match self.resolve_type_alias(exp) {
            TypeExpr::Slice { .. } | TypeExpr::Array { .. } | TypeExpr::Map { .. }
                if fields.is_empty() => {}
            TypeExpr::Named { name, .. }
                if self.is_valid_elided_struct_head_name(&name, fields.is_empty()) =>
            {
                self.check_elided_struct_fields(&name, fields);
            }
            _ => self.push_elided_diag(span),
        }
    }

    fn elided_expr(&mut self, e: &Expr, slot: Slot<'_>) {
        match e {
            // `emit_expr_exp` peels parentheses before filling the head.
            Expr::Paren { inner, .. } => self.elided_expr(inner, slot),
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.elided_type(t);
                } else {
                    self.check_elided_struct_head(slot, fields, *span);
                }
                // Field values are emitted without an expected type.
                for f in fields {
                    self.elided_expr(&f.value, Slot::Free);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.elided_type(t);
                }
                let item_slot = if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    Slot::Element(inner_elem)
                } else if let Some(t) = elem.as_deref() {
                    Slot::Element(t)
                } else {
                    match slot {
                        Slot::Element(TypeExpr::Slice { elem: inner, .. } | TypeExpr::Array { elem: inner, .. }) => {
                            Slot::Element(inner)
                        }
                        _ => Slot::Free,
                    }
                };
                for i in items {
                    self.elided_expr(i, item_slot);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.elided_type(key);
                self.elided_type(value);
                for (k, v) in entries {
                    self.elided_expr(k, Slot::Element(key));
                    self.elided_expr(v, Slot::Element(value));
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
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                params.iter().for_each(|p| self.elided_type(&p.ty));
                results.iter().for_each(|r| self.elided_type(&r.ty));
                self.elided_block(body);
            }
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
