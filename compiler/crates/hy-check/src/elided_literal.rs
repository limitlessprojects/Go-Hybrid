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

use std::collections::HashMap;

use crate::model::Checker;
use hy_diag::Diagnostic;
use hy_syntax::{
    Block, Decl, Expr, FieldInit, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr,
};

/// Curated foreign named types that are Go interfaces, function types, or
/// scalar aliases rather than structs (`go build` refuses `T{...}` with
/// `invalid composite literal type ...`).
fn is_non_struct_foreign_type(name: &str) -> bool {
    matches!(
        name,
        "io.reader"
            | "io.writer"
            | "io.closer"
            | "io.read_closer"
            | "io.write_closer"
            | "io.read_writer"
            | "io.read_write_closer"
            | "io.read_seeker"
            | "io.write_seeker"
            | "io.read_write_seeker"
            | "io.seeker"
            | "io.reader_at"
            | "io.writer_at"
            | "io.reader_from"
            | "io.writer_to"
            | "io.byte_reader"
            | "io.byte_scanner"
            | "io.byte_writer"
            | "io.rune_reader"
            | "io.rune_scanner"
            | "io.string_writer"
            | "http.handler"
            | "http.response_writer"
            | "http.flusher"
            | "http.hijacker"
            | "http.round_tripper"
            | "time.duration"
            | "os.file_mode"
            | "os.file_info"
            | "os.dir_entry"
            | "os.signal"
            | "net.conn"
            | "net.listener"
            | "net.addr"
            | "net.packet_conn"
            | "fmt.stringer"
            | "fmt.go_stringer"
            | "fmt.state"
            | "fmt.formatter"
            | "fmt.scanner"
            | "fmt.scan_state"
            | "sort.interface"
    )
}

/// Whether the expression sits in a composite element slot, where the
/// emitter inherits the head from the enclosing literal's type.
impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_literals(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => {
                    if let Some(ty) = &c.ty {
                        self.elided_type(ty);
                    }
                    self.elided_expr(&c.value, None);
                }
                Decl::Struct(s) => {
                    for f in &s.fields {
                        self.elided_type(&f.ty);
                    }
                }
                Decl::TypeAlias(a) => self.elided_type(&a.ty),
                Decl::Enum(e) => {
                    for v in &e.variants {
                        if let Some(payload) = &v.payload {
                            self.elided_type(payload);
                        }
                    }
                }
                Decl::Fn(f) => {
                    if let Some(r) = &f.receiver {
                        self.elided_type(&r.ty);
                    }
                    for p in &f.params {
                        self.elided_type(&p.ty);
                    }
                    for r in &f.results {
                        self.elided_type(&r.ty);
                    }
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
                    self.elided_expr(l, None);
                }
                self.elided_type(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.elided_type(key);
                self.elided_type(value);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.elided_type(&p.ty);
                }
                for r in results {
                    self.elided_type(r);
                }
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
        if is_empty
            && matches!(
                r,
                TypeExpr::Map { .. } | TypeExpr::Slice { .. } | TypeExpr::Array { .. }
            )
        {
            return true;
        }
        let TypeExpr::Named { name, .. } = &r else { return false };
        if name.starts_with("struct{") || name.starts_with("struct ") {
            return true;
        }
        let head = name.split('[').next().unwrap_or(name.as_str());
        if is_non_struct_foreign_type(head) {
            return false;
        }
        self.structs.contains_key(head)
            || self.pkg_names.get(head) == Some(&"struct")
            || head.contains('.')
    }

    fn check_elided_struct_fields(&mut self, exp: Option<&TypeExpr>, fields: &[FieldInit]) {
        if fields.is_empty() {
            return;
        }
        let Some(t) = exp else { return };
        let mut r = self.resolve_type_alias(t);
        if let TypeExpr::Pointer { to, .. } = r {
            r = self.resolve_type_alias(&to);
        }
        let TypeExpr::Named { name, .. } = &r else { return };

        let mut seen: HashMap<String, usize> = HashMap::new();
        for f in fields {
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
                            f.name, name, f.name
                        ),
                    )
                    .with_notes(vec![
                        "each field may be set at most once in one struct literal".to_string(),
                    ]),
                );
            }
        }

        if name.starts_with("struct{") || name.starts_with("struct ") {
            if let (Some(open), Some(close)) = (name.find('{'), name.rfind('}')) {
                if close >= open {
                    let body = &name[open + 1..close];
                    let mut inline_fields: Vec<String> = Vec::new();
                    for part in body.split(|c| c == ';' || c == '\n' || c == ',') {
                        let trimmed = part.trim();
                        if trimmed.is_empty() {
                            continue;
                        }
                        if let Some(tok) = trimmed
                            .split(|c: char| c.is_whitespace() || c == ':')
                            .find(|s| !s.is_empty())
                        {
                            inline_fields.push(tok.to_string());
                        }
                    }
                    for f in fields {
                        if !inline_fields.iter().any(|n| n == &f.name) {
                            self.diags.push(
                                Diagnostic::error(
                                    "HYB-E1002",
                                    self.path.clone(),
                                    f.span,
                                    format!(
                                        "unknown field `{}` in `{}` literal — Go refuses `unknown field {} in struct literal of type {}` (HYB-E1002)",
                                        f.name, name, f.name, name
                                    ),
                                )
                                .with_notes(vec![
                                    "a struct literal may only name fields the struct declares".to_string(),
                                ]),
                            );
                        }
                    }
                }
            }
            return;
        }

        let head = name.split('[').next().unwrap_or(name.as_str());
        let declared = if let Some(d) = self.structs.get(head).cloned() {
            d
        } else if self.pkg_names.get(head) == Some(&"struct") {
            Vec::new()
        } else {
            return;
        };

        for f in fields {
            if !declared.iter().any(|(n, _)| n == &f.name) {
                self.diags.push(
                    Diagnostic::error(
                        "HYB-E1002",
                        self.path.clone(),
                        f.span,
                        format!(
                            "unknown field `{}` in `{}` literal — Go refuses `unknown field {} in struct literal of type {}` (HYB-E1002)",
                            f.name, name, f.name, name
                        ),
                    )
                    .with_notes(vec![
                        "a struct literal may only name fields the struct declares".to_string(),
                    ]),
                );
            }
        }
    }

    fn elided_expr(&mut self, e: &Expr, expected: Option<&TypeExpr>) {
        match e {
            // `emit_expr_exp` peels parentheses before filling the head.
            Expr::Paren { inner, .. } => self.elided_expr(inner, expected),
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.elided_type(t);
                } else if !self.is_elidable_struct_slot(expected, fields.is_empty()) {
                    self.push_elided_diag(*span);
                } else {
                    self.check_elided_struct_fields(expected, fields);
                }
                // Field values are emitted without an expected type.
                for f in fields {
                    self.elided_expr(&f.value, None);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.elided_type(t);
                }
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
                self.elided_type(key);
                self.elided_type(value);
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
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.elided_type(&p.ty);
                }
                for r in results {
                    self.elided_type(&r.ty);
                }
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
