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
            | "context.context"
            | "context.canceler"
            | "context.cancel_func"
            | "sync.locker"
            | "http.handler_func"
            | "filepath.walk_func"
            | "bufio.split_func"
            | "rand.source"
            | "rand.source64"
            | "hash.hash"
            | "hash.hash32"
            | "hash.hash64"
            | "cipher.block"
            | "cipher.stream"
            | "cipher.aead"
            | "cipher.block_mode"
            | "reflect.type"
            | "reflect.kind"
            | "flag.value"
            | "flag.getter"
            | "testing.tb"
            | "sql.scanner"
            | "sql.result"
    )
}

fn collect_receiver_tps(ty: &TypeExpr, out: &mut Vec<String>) {
    match ty {
        TypeExpr::Pointer { to, .. } => collect_receiver_tps(to, out),
        TypeExpr::Named { name, .. } => {
            if let Some((_, rest)) = name.split_once('[') {
                if let Some(inner) = rest.strip_suffix(']') {
                    for part in inner.split(',') {
                        let tp = part.trim();
                        if !tp.is_empty() && tp != "_" {
                            out.push(tp.to_string());
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Whether the expression sits in a composite element slot, where the
/// emitter inherits the head from the enclosing literal's type.
impl<'a> Checker<'a> {
    pub(crate) fn check_elided_struct_literals(&mut self) {
        let file = self.file;
        let empty_tps: Vec<String> = Vec::new();
        for d in &file.decls {
            match d {
                Decl::Const(c) => {
                    if let Some(ty) = &c.ty {
                        self.elided_type(ty, &empty_tps);
                    }
                    self.elided_expr(&c.value, None, &empty_tps);
                }
                Decl::Struct(s) => {
                    let tps: Vec<String> = s
                        .type_params
                        .iter()
                        .map(|tp| tp.name.clone())
                        .filter(|n| !n.is_empty() && n != "_")
                        .collect();
                    for f in &s.fields {
                        self.elided_type(&f.ty, &tps);
                    }
                }
                Decl::TypeAlias(a) => self.elided_type(&a.ty, &empty_tps),
                Decl::Enum(e) => {
                    for v in &e.variants {
                        if let Some(payload) = &v.payload {
                            self.elided_type(payload, &empty_tps);
                        }
                    }
                }
                Decl::Fn(f) => {
                    let mut tps: Vec<String> = f
                        .type_params
                        .iter()
                        .map(|tp| tp.name.clone())
                        .filter(|n| !n.is_empty() && n != "_")
                        .collect();
                    if let Some(r) = &f.receiver {
                        collect_receiver_tps(&r.ty, &mut tps);
                        self.elided_type(&r.ty, &tps);
                    }
                    for p in &f.params {
                        self.elided_type(&p.ty, &tps);
                    }
                    for r in &f.results {
                        self.elided_type(&r.ty, &tps);
                    }
                    if let Some(b) = &f.body {
                        self.elided_block(b, &tps);
                    }
                }
                Decl::Import { .. } => {}
            }
        }
    }

    fn elided_type(&mut self, ty: &TypeExpr, tps: &[String]) {
        match ty {
            TypeExpr::Named { .. } => {}
            TypeExpr::Pointer { to, .. }
            | TypeExpr::Slice { elem: to, .. }
            | TypeExpr::Chan { elem: to, .. }
            | TypeExpr::Option { elem: to, .. } => self.elided_type(to, tps),
            TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.elided_expr(l, None, tps);
                }
                self.elided_type(elem, tps);
            }
            TypeExpr::Map { key, value, .. } => {
                self.elided_type(key, tps);
                self.elided_type(value, tps);
            }
            TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.elided_type(&p.ty, tps);
                }
                for r in results {
                    self.elided_type(r, tps);
                }
            }
        }
    }

    fn elided_block(&mut self, b: &Block, tps: &[String]) {
        for s in &b.stmts {
            self.elided_stmt(s, tps);
        }
    }

    fn elided_stmt(&mut self, s: &Stmt, tps: &[String]) {
        match s {
            Stmt::Local(l) => {
                if let Some(ty) = &l.ty {
                    self.elided_type(ty, tps);
                }
                for v in &l.values {
                    self.elided_expr(v, None, tps);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.elided_place(t, tps);
                }
                for v in values {
                    self.elided_expr(v, None, tps);
                }
            }
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
                self.elided_expr(expr, None, tps)
            }
            Stmt::Send { chan, value, .. } => {
                self.elided_expr(chan, None, tps);
                self.elided_expr(value, None, tps);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if let Some(i) = init {
                    self.elided_stmt(i, tps);
                }
                self.elided_expr(cond, None, tps);
                self.elided_block(then, tps);
                if let Some(o) = otherwise {
                    self.elided_stmt(o, tps);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::Infinite => {}
                    ForKind::While { cond } => self.elided_expr(cond, None, tps),
                    ForKind::Range { lo, hi, .. } => {
                        self.elided_expr(lo, None, tps);
                        self.elided_expr(hi, None, tps);
                    }
                    ForKind::Iterate { expr, .. } => self.elided_expr(expr, None, tps),
                }
                self.elided_block(body, tps);
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.elided_expr(v, None, tps);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.elided_expr(callee, None, tps);
                for a in args {
                    self.elided_expr(&a.value, None, tps);
                }
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                self.elided_block(body, tps)
            }
            Stmt::Match { subject, arms, .. } => {
                self.elided_expr(subject, None, tps);
                for a in arms {
                    if let Some(g) = &a.guard {
                        self.elided_expr(g, None, tps);
                    }
                    self.elided_block(&a.body, tps);
                }
            }
            Stmt::Select { arms, .. } => {
                for a in arms {
                    match &a.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.elided_expr(chan, None, tps);
                            self.elided_expr(value, None, tps);
                        }
                        SelectArmKind::Recv { chan, .. } => self.elided_expr(chan, None, tps),
                        SelectArmKind::Default => {}
                    }
                    self.elided_block(&a.body, tps);
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn elided_place(&mut self, p: &Place, tps: &[String]) {
        match &p.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.elided_place(base, tps);
                self.elided_expr(index, None, tps);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.elided_place(base, tps)
            }
        }
    }

    fn resolve_elided_type(&self, ty: &TypeExpr, tps: &[String]) -> TypeExpr {
        let mut cur = ty.clone();
        for _ in 0..8 {
            let TypeExpr::Named { name, .. } = &cur else {
                break;
            };
            let head = name.split('[').next().unwrap_or(name.as_str());
            if tps.iter().any(|tp| tp == head) {
                break;
            }
            if let Some(next) = self.type_aliases.get(name.as_str()) {
                cur = next.clone();
            } else {
                break;
            }
        }
        cur
    }

    fn is_elidable_struct_slot(&self, exp: Option<&TypeExpr>, is_empty: bool, tps: &[String]) -> bool {
        let Some(t) = exp else { return false };
        let mut r = self.resolve_elided_type(t, tps);
        if let TypeExpr::Pointer { to, .. } = r {
            r = self.resolve_elided_type(&to, tps);
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
        if tps.iter().any(|tp| tp == head) || is_non_struct_foreign_type(head) {
            return false;
        }
        self.structs.contains_key(head) || head.contains('.')
    }

    fn check_elided_struct_fields(&mut self, exp: Option<&TypeExpr>, fields: &[FieldInit], tps: &[String]) {
        if fields.is_empty() {
            return;
        }
        let Some(t) = exp else { return };
        let mut r = self.resolve_elided_type(t, tps);
        if let TypeExpr::Pointer { to, .. } = r {
            r = self.resolve_elided_type(&to, tps);
        }
        let TypeExpr::Named { name, .. } = &r else { return };
        let head = name.split('[').next().unwrap_or(name.as_str());

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

        let Some(declared) = self.structs.get(head).cloned() else {
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

    fn elided_expr(&mut self, e: &Expr, expected: Option<&TypeExpr>, tps: &[String]) {
        match e {
            // `emit_expr_exp` peels parentheses before filling the head.
            Expr::Paren { inner, .. } => self.elided_expr(inner, expected, tps),
            Expr::StructLit { ty, fields, span } => {
                if let Some(t) = ty {
                    self.elided_type(t, tps);
                } else if !self.is_elidable_struct_slot(expected, fields.is_empty(), tps) {
                    self.push_elided_diag(*span);
                } else {
                    self.check_elided_struct_fields(expected, fields, tps);
                }
                // Field values are emitted without an expected type.
                for f in fields {
                    self.elided_expr(&f.value, None, tps);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.elided_type(t, tps);
                }
                let resolved_exp = expected.map(|t| self.resolve_elided_type(t, tps));
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
                    self.elided_expr(i, item_exp, tps);
                }
            }
            Expr::MapLit { key, value, entries, .. } => {
                self.elided_type(key, tps);
                self.elided_type(value, tps);
                for (k, v) in entries {
                    self.elided_expr(k, Some(key), tps);
                    self.elided_expr(v, Some(value), tps);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(x) = p {
                        self.elided_expr(x, None, tps);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.elided_expr(lhs, None, tps);
                self.elided_expr(rhs, None, tps);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.elided_expr(operand, None, tps),
            Expr::Receive { chan, .. } => self.elided_expr(chan, None, tps),
            Expr::Index { base, index, .. } => {
                self.elided_expr(base, None, tps);
                self.elided_expr(index, None, tps);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.elided_expr(base, None, tps);
                if let Some(l) = lo {
                    self.elided_expr(l, None, tps);
                }
                if let Some(h) = hi {
                    self.elided_expr(h, None, tps);
                }
            }
            Expr::Field { base, .. } => self.elided_expr(base, None, tps),
            Expr::Call { callee, args, .. } => {
                self.elided_expr(callee, None, tps);
                for a in args {
                    self.elided_expr(&a.value, None, tps);
                }
            }
            Expr::FnLit {
                params,
                results,
                body,
                ..
            } => {
                for p in params {
                    self.elided_type(&p.ty, tps);
                }
                for r in results {
                    self.elided_type(&r.ty, tps);
                }
                self.elided_block(body, tps);
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
