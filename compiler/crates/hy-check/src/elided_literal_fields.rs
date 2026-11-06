//! elided_literal_fields.rs — Issue #3743 (GUARANTEES Row 13):
//!   1. Unknown (`U14`) and duplicate (`U15`) field names inside type-elided
//!      struct literals `{f: v}` in composite element slots (`[]point{{nope: 1}}`,
//!      `[]*point{{x: 1, x: 2}}`, `map[string]point{"a": {nope: 1}}`,
//!      `[][]point{[{nope: 1}]}`).
//!   2. Type-elided struct literals inside `[N]T` array length expressions
//!      (`TypeExpr::Array.len`) across declarations, statements, and expressions.
//!   3. Type-elided `{}` in curated non-struct foreign slots (`io.reader`,
//!      `http.handler`, `http.response_writer`, `time.duration`, `os.file_mode`,
//!      `net.conn`, etc.).

use std::collections::HashMap;

use crate::model::Checker;
use hy_diag::Diagnostic;
use hy_syntax::{
    Block, Decl, Expr, FieldInit, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr,
};

fn is_non_struct_foreign_head(head: &str) -> bool {
    matches!(
        head,
        "io.reader"
            | "io.writer"
            | "io.closer"
            | "io.read_closer"
            | "io.write_closer"
            | "io.read_writer"
            | "io.read_write_closer"
            | "io.seeker"
            | "io.read_seeker"
            | "io.write_seeker"
            | "io.read_write_seeker"
            | "io.read_seek_closer"
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
            | "http.handler_func"
            | "http.response_writer"
            | "http.flusher"
            | "http.hijacker"
            | "http.close_notifier"
            | "http.pusher"
            | "time.duration"
            | "time.month"
            | "time.weekday"
            | "os.file_mode"
            | "os.signal"
            | "fs.file"
            | "fs.file_info"
            | "fs.dir_entry"
            | "fs.file_mode"
            | "fs.fs"
            | "fs.walk_dir_func"
            | "net.conn"
            | "net.packet_conn"
            | "net.listener"
            | "net.addr"
            | "net.error"
            | "context.context"
            | "context.cancel_func"
            | "sort.interface"
            | "fmt.stringer"
            | "hash.hash"
            | "hash.hash32"
            | "hash.hash64"
            | "sql.isolation_level"
            | "bufio.split_func"
            | "filepath.walk_func"
    )
}

impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_fields_and_slots(&mut self) {
        let file = self.file;
        for d in &file.decls {
            match d {
                Decl::Const(c) => {
                    if let Some(t) = &c.ty {
                        self.elided_slot_type(t);
                    }
                    self.elided_slot_expr(&c.value, None);
                }
                Decl::Struct(s) => {
                    for f in &s.fields {
                        self.elided_slot_type(&f.ty);
                    }
                }
                Decl::TypeAlias(ta) => {
                    self.elided_slot_type(&ta.ty);
                }
                Decl::Enum(e) => {
                    for v in &e.variants {
                        if let Some(p) = &v.payload {
                            self.elided_slot_type(p);
                        }
                    }
                }
                Decl::Fn(f) => {
                    if let Some(r) = &f.receiver {
                        self.elided_slot_type(&r.ty);
                    }
                    for p in &f.params {
                        self.elided_slot_type(&p.ty);
                    }
                    for res in &f.results {
                        self.elided_slot_type(&res.ty);
                    }
                    if let Some(b) = &f.body {
                        self.elided_slot_block(b);
                    }
                }
                Decl::Import { .. } => {}
            }
        }
    }

    fn elided_slot_type(&mut self, t: &TypeExpr) {
        match t {
            TypeExpr::Named { .. } => {}
            TypeExpr::Pointer { to, .. } => self.elided_slot_type(to),
            TypeExpr::Slice { elem, .. }
            | TypeExpr::Chan { elem, .. }
            | TypeExpr::Option { elem, .. } => self.elided_slot_type(elem),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.elided_slot_expr(l, None);
                }
                self.elided_slot_type(elem);
            }
            TypeExpr::Map { key, value, .. } => {
                self.elided_slot_type(key);
                self.elided_slot_type(value);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.elided_slot_type(&p.ty);
                }
                for r in results {
                    self.elided_slot_type(r);
                }
            }
        }
    }

    fn elided_slot_block(&mut self, b: &Block) {
        for s in &b.stmts {
            self.elided_slot_stmt(s);
        }
    }

    fn elided_slot_stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Local(l) => {
                if let Some(t) = &l.ty {
                    self.elided_slot_type(t);
                }
                for v in &l.values {
                    self.elided_slot_expr(v, None);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.elided_slot_place(t);
                }
                for v in values {
                    self.elided_slot_expr(v, None);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.elided_slot_expr(expr, None)
            }
            Stmt::Send { chan, value, .. } => {
                self.elided_slot_expr(chan, None);
                self.elided_slot_expr(value, None);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.elided_slot_stmt(i);
                }
                self.elided_slot_expr(cond, None);
                self.elided_slot_block(then);
                if let Some(o) = otherwise {
                    self.elided_slot_stmt(o);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.elided_slot_expr(cond, None),
                    ForKind::Range { lo, hi, .. } => {
                        self.elided_slot_expr(lo, None);
                        self.elided_slot_expr(hi, None);
                    }
                    ForKind::Iterate { expr, .. } => self.elided_slot_expr(expr, None),
                }
                self.elided_slot_block(body);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.elided_slot_expr(v, None);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.elided_slot_expr(callee, None);
                for a in args {
                    self.elided_slot_expr(&a.value, None);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                self.elided_slot_block(body)
            }
            Stmt::Match { subject, arms, .. } => {
                self.elided_slot_expr(subject, None);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.elided_slot_expr(g, None);
                    }
                    self.elided_slot_block(&a.body);
                }
            }
            Stmt::Select { arms, .. } => {
                for a in arms {
                    match &a.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.elided_slot_expr(chan, None);
                            self.elided_slot_expr(value, None);
                        }
                        SelectArmKind::Recv { chan, .. } => self.elided_slot_expr(chan, None),
                        SelectArmKind::Default => {}
                    }
                    self.elided_slot_block(&a.body);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn elided_slot_place(&mut self, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.elided_slot_place(base);
                self.elided_slot_expr(index, None);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.elided_slot_place(base)
            }
        }
    }

    fn resolve_elidable_slot_type(&self, exp: Option<&TypeExpr>, is_empty: bool) -> (bool, Option<String>) {
        let Some(t) = exp else {
            return (false, None);
        };
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
            return (true, None);
        }
        let TypeExpr::Named { name, .. } = &r else {
            return (false, None);
        };
        if name.starts_with("struct{") || name.starts_with("struct ") {
            return (true, None);
        }
        let head = name.split('[').next().unwrap_or(name.as_str());
        if self.structs.contains_key(head) {
            return (true, Some(name.clone()));
        }
        if head.contains('.') && !is_non_struct_foreign_head(head) {
            return (true, None);
        }
        (false, None)
    }

    fn check_elided_struct_lit_fields(&mut self, full_name: &str, fields: &[FieldInit]) {
        let sname = full_name.split('[').next().unwrap_or(full_name).to_string();
        let Some(declared) = self.structs.get(&sname).cloned() else {
            return;
        };
        // U15: duplicate field in one literal.
        let mut seen: HashMap<String, usize> = HashMap::new();
        for f in fields {
            let n = seen.entry(f.name.clone()).or_insert(0);
            *n += 1;
            if *n == 2 {
                self.push_elided_field_diag(
                    f.span,
                    format!(
                        "duplicate field `{}` in `{}` literal — Go refuses `duplicate field name {} in struct literal` (HYB-E1002)",
                        f.name, full_name, f.name
                    ),
                    "each field may be set at most once in one struct literal",
                );
            }
        }
        // U14: unknown field name.
        for f in fields {
            if !declared.iter().any(|(n, _)| n == &f.name) {
                self.push_elided_field_diag(
                    f.span,
                    format!(
                        "unknown field `{}` in `{}` literal — Go refuses `unknown field {} in struct literal of type {}` (HYB-E1002)",
                        f.name, full_name, f.name, full_name
                    ),
                    "a struct literal may only name fields the struct declares",
                );
            }
        }
    }

    fn elided_slot_expr(&mut self, e: &Expr, expected: Option<&TypeExpr>) {
        match e {
            Expr::Paren { inner, .. } => self.elided_slot_expr(inner, expected),
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.elided_slot_type(t);
                } else {
                    let (ok, struct_full_name) =
                        self.resolve_elidable_slot_type(expected, fields.is_empty());
                    if !ok {
                        self.push_elided_slot_diag(*span);
                    } else if let Some(full_name) = struct_full_name {
                        self.check_elided_struct_lit_fields(&full_name, fields);
                    }
                }
                for f in fields {
                    self.elided_slot_expr(&f.value, None);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.elided_slot_type(t);
                }
                let resolved_exp = expected.map(|t| self.resolve_type_alias(t));
                let item_exp = if let Some((_, inner_elem, _)) = e.as_array_lit() {
                    Some(inner_elem)
                } else if let Some(t) = elem.as_deref() {
                    Some(t)
                } else {
                    match &resolved_exp {
                        Some(
                            TypeExpr::Slice { elem: inner, .. }
                            | TypeExpr::Array { elem: inner, .. },
                        ) => Some(inner.as_ref()),
                        _ => None,
                    }
                };
                for i in items {
                    self.elided_slot_expr(i, item_exp);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.elided_slot_type(key);
                self.elided_slot_type(value);
                for (k, v) in entries {
                    self.elided_slot_expr(k, Some(key));
                    self.elided_slot_expr(v, Some(value));
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.elided_slot_expr(x, None);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.elided_slot_expr(lhs, None);
                self.elided_slot_expr(rhs, None);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.elided_slot_expr(operand, None),
            Expr::Receive { chan, .. } => self.elided_slot_expr(chan, None),
            Expr::Index { base, index, .. } => {
                self.elided_slot_expr(base, None);
                self.elided_slot_expr(index, None);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.elided_slot_expr(base, None);
                if let Some(l) = lo {
                    self.elided_slot_expr(l, None);
                }
                if let Some(h) = hi {
                    self.elided_slot_expr(h, None);
                }
            }
            Expr::Field { base, .. } => self.elided_slot_expr(base, None),
            Expr::Call { callee, args, .. } => {
                self.elided_slot_expr(callee, None);
                for a in args {
                    self.elided_slot_expr(&a.value, None);
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.elided_slot_type(&p.ty);
                }
                for r in results {
                    self.elided_slot_type(&r.ty);
                }
                self.elided_slot_block(body);
            }
        }
    }

    fn push_elided_slot_diag(&mut self, span: hy_diag::Span) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.span == span)
        {
            return;
        }
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

    fn push_elided_field_diag(&mut self, span: hy_diag::Span, msg: String, note: &str) {
        if self
            .diags
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.span == span)
        {
            return;
        }
        self.diags.push(
            Diagnostic::error("HYB-E1002", self.path.clone(), span, msg)
                .with_notes(vec![note.to_string()]),
        );
    }
}
