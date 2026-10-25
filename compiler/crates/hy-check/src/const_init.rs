//! const_init.rs — `const` initializers that are not constant expressions
//! (issue #205).
//!
//! `const c = get_val()` passes `hyb check` with 0 diagnostics today, then
//! fails `go build` with `get_val() (value of type int) is not constant`.
//! The emitter keeps the Go `const` keyword for every non-composite
//! initializer (`hy-emit` stmt.rs), so the accepted-should-emit-buildable-Go
//! contract requires a checker diagnostic (HYB-E6003).
//!
//! Composite literals (`[]T{…}`, maps, structs) already lower to Go `var`
//! and are intentionally out of scope here — their runtime-mutable /
//! sendability story is issue #188.

use crate::model::{unwrap_parens, Checker, FnState};
use hy_diag::{Diagnostic, Pos, Span};
use hy_syntax::{
    BinOp, Block, Decl, Expr, ForKind, InterpPart, LitKind, LocalKind, MatchPattern, SelectArmKind,
    Stmt, TypeExpr, UnOp,
};

#[derive(Clone, Copy)]
enum LocalConstLookup<'a> {
    Const { ty: Option<&'a TypeExpr>, value: &'a Expr },
    NonConst,
}

impl<'a> Checker<'a> {
    /// When `value` would be emitted as a Go `const` initializer but is not
    /// a constant expression, emit HYB-E6003.
    pub(crate) fn check_const_init_is_constant(
        &mut self,
        name: &str,
        value: &Expr,
        st: Option<&FnState>,
    ) {
        if is_composite_lit(value) {
            // Emitter rewrites these to `var` — not the #205 contract break.
            return;
        }
        let decl_ty = if st.is_none() {
            self.file_const_decl_at(name, value.span()).and_then(|c| c.ty.as_ref())
        } else {
            match self.resolve_local_const(name, value.span().start, Some(value.span())) {
                Some(LocalConstLookup::Const { ty, .. }) => ty,
                _ => None,
            }
        };
        let ty_ok = decl_ty.is_none_or(|ty| self.is_const_scalar_or_str_type(ty));
        let mut visiting = Vec::new();
        if st.is_none() {
            visiting.push(name.to_string());
        }
        if ty_ok && self.expr_is_constant_inner(value, st, &mut visiting, 0) {
            return;
        }
        let span = value.span();
        let msg = format!(
            "const `{}` initializer `{}` is not a constant expression (HYB-E6003)",
            name,
            value.render()
        );
        let notes = vec![
            "why: Go rejects a non-constant `const` initializer (`… is not constant`), so an accepted Hybrid program would emit unbuildable Go (issue #205)".to_string(),
            "fix: use a literal or constant expression (`const c = 42`, `const c = 1 + 2`), reference another `const`, or bind a run-time value with `:=` / `let` / `var`".to_string(),
            format!("note: declared here ({})", span_hint(span)),
        ];
        self.diags.push(
            Diagnostic::error("HYB-E6003", self.path.clone(), span, msg).with_notes(notes),
        );
    }

    /// Compile-time constant expression under the narrow MVP rules that
    /// match what Go's `const` accepts for the shapes Hybrid emits.
    fn expr_is_constant_inner(
        &self,
        e: &Expr,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> bool {
        if ref_depth > 32 {
            return false;
        }
        match unwrap_parens(e) {
            Expr::Lit { kind, .. } => !matches!(kind, LitKind::Nil),
            Expr::Unary {
                op: UnOp::Neg | UnOp::Not,
                operand,
                ..
            } => self.expr_is_constant_inner(operand, st, visiting, ref_depth),
            Expr::Unary { op: UnOp::Addr, .. } => false,
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_is_constant_inner(lhs, st, visiting, ref_depth)
                    && self.expr_is_constant_inner(rhs, st, visiting, ref_depth)
            }
            Expr::Ident { name, span } => {
                self.ident_is_constant(name, span.start, st, visiting, ref_depth + 1)
            }
            // Typed conversion of a constant (`int8(42)`, `byte(0xff)`) or
            // `len("const_str")`. A call to a real function (`get_val()`) or
            // shadowed local binding is never constant.
            Expr::Call { callee, args, .. } => {
                let Some(ty_name) = conversion_callee_name(callee) else {
                    return false;
                };
                if self.is_callee_shadowed(ty_name, callee.span().start, st)
                    || args.len() != 1
                    || args[0].spread
                    || args[0].name.is_some()
                {
                    return false;
                }
                if ty_name == "len" {
                    return !self.type_aliases.contains_key("len")
                        && self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth)
                        && self.is_const_string_expr(&args[0].value, st, ref_depth);
                }
                self.is_const_conversion_target(ty_name)
                    && self.expr_is_constant_inner(&args[0].value, st, visiting, ref_depth)
            }
            // Everything else (field calls, index, receive, try, interp,
            // move, addr-of, composites nested under non-composite forms…)
            // is run-time.
            _ => false,
        }
    }

    fn is_callee_shadowed(&self, name: &str, callee_pos: Pos, st: Option<&FnState>) -> bool {
        (st.is_some() && self.resolve_local_const(name, callee_pos, None).is_some())
            || self.fns.contains_key(name)
            || self.structs.contains_key(name)
            || self.enums.contains_key(name)
            || self.enums.values().any(|vs| vs.iter().any(|v| v == name))
            || self.file_const_decl(name).is_some()
            || self.packages.contains(name)
    }

    fn is_const_scalar_or_str_type(&self, ty: &TypeExpr) -> bool {
        matches!(self.resolve_type_alias(ty), TypeExpr::Named { name: t, .. }
            if !self.type_aliases.contains_key(t.as_str())
                && !self.structs.contains_key(t.as_str())
                && !self.enums.contains_key(t.as_str())
                && is_conversion_type_name(&t))
    }

    fn is_const_str_type(&self, ty: &TypeExpr) -> bool {
        matches!(self.resolve_type_alias(ty), TypeExpr::Named { name: t, .. }
            if t == "string"
                && !self.type_aliases.contains_key("string")
                && !self.structs.contains_key("string")
                && !self.enums.contains_key("string"))
    }

    fn is_const_conversion_target(&self, name: &str) -> bool {
        self.type_aliases.get(name).map_or_else(
            || !self.structs.contains_key(name) && !self.enums.contains_key(name) && is_conversion_type_name(name),
            |ty| self.is_const_scalar_or_str_type(ty),
        )
    }

    fn is_const_string_conversion_target(&self, name: &str, pos: Pos, st: Option<&FnState>) -> bool {
        !self.is_callee_shadowed(name, pos, st)
            && self.type_aliases.get(name).map_or(name == "string", |ty| self.is_const_str_type(ty))
    }

    fn is_const_string_expr(&self, e: &Expr, st: Option<&FnState>, ref_depth: usize) -> bool {
        if ref_depth > 32 {
            return false;
        }
        match unwrap_parens(e) {
            Expr::Lit {
                kind: LitKind::Str(_) | LitKind::RawStr(_),
                ..
            } => true,
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                self.is_const_string_expr(lhs, st, ref_depth)
                    && self.is_const_string_expr(rhs, st, ref_depth)
            }
            Expr::Ident { name, span } => {
                if st.is_some() {
                    match self.resolve_local_const(name, span.start, None) {
                        Some(LocalConstLookup::NonConst) => return false,
                        Some(LocalConstLookup::Const { ty, value }) => {
                            return !is_composite_lit(value)
                                && ty.is_none_or(|t| self.is_const_str_type(t))
                                && self.is_const_string_expr(value, st, ref_depth + 1);
                        }
                        None => {}
                    }
                }
                self.file_const_decl(name).is_some_and(|cdecl| {
                    !is_composite_lit(&cdecl.value)
                        && cdecl.ty.as_ref().is_none_or(|t| self.is_const_str_type(t))
                        && self.is_const_string_expr(&cdecl.value, None, ref_depth + 1)
                })
            }
            Expr::Call { callee, args, .. } => {
                conversion_callee_name(callee).is_some_and(|ty_name| {
                    self.is_const_string_conversion_target(ty_name, callee.span().start, st)
                        && args.len() == 1
                        && !args[0].spread
                        && args[0].name.is_none()
                })
            }
            _ => false,
        }
    }

    fn file_const_decl(&self, name: &str) -> Option<&hy_syntax::ConstDecl> {
        self.file.decls.iter().find_map(|d| match d {
            Decl::Const(c) if c.name == name => Some(c),
            _ => None,
        })
    }

    fn file_const_decl_at(&self, name: &str, val_span: Span) -> Option<&hy_syntax::ConstDecl> {
        self.file.decls.iter().find_map(|d| match d {
            Decl::Const(c) if c.name == name && c.value.span() == val_span => Some(c),
            _ => None,
        }).or_else(|| self.file_const_decl(name))
    }

    fn ident_is_constant(
        &self,
        name: &str,
        use_pos: Pos,
        st: Option<&FnState>,
        visiting: &mut Vec<String>,
        ref_depth: usize,
    ) -> bool {
        if ref_depth > 32 {
            return false;
        }
        // Local bindings and parameters shadow file-level constants.
        if st.is_some() {
            match self.resolve_local_const(name, use_pos, None) {
                Some(LocalConstLookup::NonConst) => return false,
                Some(LocalConstLookup::Const { ty, value }) => {
                    return !is_composite_lit(value)
                        && ty.is_none_or(|t| self.is_const_scalar_or_str_type(t))
                        && self.expr_is_constant_inner(value, st, visiting, ref_depth);
                }
                None => {}
            }
        }
        if visiting.iter().any(|v| v == name) {
            return false;
        }
        if let Some(cdecl) = self.file_const_decl(name) {
            if is_composite_lit(&cdecl.value)
                || cdecl.ty.as_ref().is_some_and(|t| !self.is_const_scalar_or_str_type(t))
            {
                return false;
            }
            visiting.push(name.to_string());
            let ok = self.expr_is_constant_inner(&cdecl.value, None, visiting, ref_depth);
            visiting.pop();
            return ok;
        }
        false
    }

    fn resolve_local_const(&self, name: &str, use_pos: Pos, self_sp: Option<Span>) -> Option<LocalConstLookup<'a>> {
        for decl in &self.file.decls {
            if let Decl::Fn(f) = decl {
                if span_contains(f.span, use_pos) {
                    let mut found = (f.params.iter().any(|p| p.name == name)
                        || f.results.iter().any(|r| r.name.as_deref() == Some(name)))
                    .then_some(LocalConstLookup::NonConst);
                    if let Some(body) = &f.body {
                        Self::scan_block_local_const(body, name, use_pos, self_sp, &mut found);
                    }
                    return found;
                }
            }
        }
        None
    }

    fn scan_block_local_const(block: &'a Block, name: &str, use_pos: Pos, self_sp: Option<Span>, found: &mut Option<LocalConstLookup<'a>>) {
        if !span_contains(block.span, use_pos) {
            return;
        }
        for stmt in &block.stmts {
            let sp = stmt_span(stmt);
            if span_contains(sp, use_pos) {
                Self::scan_stmt_local_const(stmt, name, use_pos, self_sp, found);
                return;
            }
            if pos_le(sp.end, use_pos) {
                Self::apply_local_const_stmt(stmt, name, found);
            }
        }
    }

    fn apply_local_const_stmt(stmt: &'a Stmt, name: &str, found: &mut Option<LocalConstLookup<'a>>) {
        if let Stmt::Local(l) = stmt {
            if let Some(idx) = l.names.iter().position(|n| n == name) {
                *found = Some(match (l.kind, l.values.get(idx)) {
                    (LocalKind::Const, Some(value)) => LocalConstLookup::Const { ty: l.ty.as_ref(), value },
                    _ => LocalConstLookup::NonConst,
                });
            }
        }
    }

    fn scan_stmt_local_const(stmt: &'a Stmt, name: &str, use_pos: Pos, self_sp: Option<Span>, found: &mut Option<LocalConstLookup<'a>>) {
        let scan_e = |e: &'a Expr, found: &mut Option<LocalConstLookup<'a>>| {
            Self::scan_expr_local_const(e, name, use_pos, self_sp, found);
        };
        match stmt {
            Stmt::Local(l) => {
                if let (Some(sp), LocalKind::Const) = (self_sp, l.kind) {
                    if let Some((_, v)) = l.names.iter().zip(&l.values).find(|(n, v)| n.as_str() == name && v.span() == sp) {
                        *found = Some(LocalConstLookup::Const { ty: l.ty.as_ref(), value: v });
                        return;
                    }
                }
                l.values.iter().for_each(|v| scan_e(v, found));
            }
            Stmt::Assign { values, .. } | Stmt::Return { values, .. } => values.iter().for_each(|v| scan_e(v, found)),
            Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => scan_e(expr, found),
            Stmt::Send { chan, value, .. } => {
                scan_e(chan, found);
                scan_e(value, found);
            }
            Stmt::Task { callee, args, .. } => {
                scan_e(callee, found);
                args.iter().for_each(|a| scan_e(&a.value, found));
            }
            Stmt::If { init, cond, then, otherwise, .. } => {
                if let Some(init_stmt) = init {
                    let sp = stmt_span(init_stmt);
                    if span_contains(sp, use_pos) {
                        Self::scan_stmt_local_const(init_stmt, name, use_pos, self_sp, found);
                        return;
                    }
                    if pos_le(sp.end, use_pos) {
                        Self::apply_local_const_stmt(init_stmt, name, found);
                    }
                }
                scan_e(cond, found);
                Self::scan_block_local_const(then, name, use_pos, self_sp, found);
                if let Some(other) = otherwise.as_deref().filter(|o| span_contains(stmt_span(o), use_pos)) {
                    Self::scan_stmt_local_const(other, name, use_pos, self_sp, found);
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => scan_e(cond, found),
                    ForKind::Range { var, lo, hi, .. } => {
                        scan_e(lo, found);
                        scan_e(hi, found);
                        if var == name && span_contains(body.span, use_pos) {
                            *found = Some(LocalConstLookup::NonConst);
                        }
                    }
                    ForKind::Iterate { vars, expr } => {
                        scan_e(expr, found);
                        if vars.iter().any(|v| v == name) && span_contains(body.span, use_pos) {
                            *found = Some(LocalConstLookup::NonConst);
                        }
                    }
                    ForKind::Infinite => {}
                }
                Self::scan_block_local_const(body, name, use_pos, self_sp, found);
            }
            Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
                Self::scan_block_local_const(body, name, use_pos, self_sp, found);
            }
            Stmt::Match { subject, arms, .. } => {
                scan_e(subject, found);
                for arm in arms {
                    if arm.guard.as_ref().is_some_and(|g| span_contains(g.span(), use_pos)) || span_contains(arm.body.span, use_pos) {
                        if matches!(&arm.pattern, MatchPattern::Path { binding: Some(b), .. } if b == name) {
                            *found = Some(LocalConstLookup::NonConst);
                        }
                        arm.guard.iter().for_each(|g| scan_e(g, found));
                        Self::scan_block_local_const(&arm.body, name, use_pos, self_sp, found);
                        return;
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            scan_e(chan, found);
                            scan_e(value, found);
                        }
                        SelectArmKind::Recv { chan, .. } => scan_e(chan, found),
                        SelectArmKind::Default => {}
                    }
                    if span_contains(arm.body.span, use_pos) {
                        if matches!(&arm.kind, SelectArmKind::Recv { names, .. } if names.iter().any(|n| n == name)) {
                            *found = Some(LocalConstLookup::NonConst);
                        }
                        Self::scan_block_local_const(&arm.body, name, use_pos, self_sp, found);
                        return;
                    }
                }
            }
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
        }
    }

    fn scan_expr_local_const(expr: &'a Expr, name: &str, use_pos: Pos, self_sp: Option<Span>, found: &mut Option<LocalConstLookup<'a>>) {
        if !span_contains(expr.span(), use_pos) {
            return;
        }
        let scan_e = |e: &'a Expr, found: &mut Option<LocalConstLookup<'a>>| {
            Self::scan_expr_local_const(e, name, use_pos, self_sp, found);
        };
        match expr {
            Expr::FnLit { params, results, body, .. } => {
                if params.iter().any(|p| span_contains(p.ty.span(), use_pos))
                    || results.iter().any(|r| span_contains(r.ty.span(), use_pos))
                {
                    return;
                }
                if params.iter().any(|p| p.name == name) || results.iter().any(|r| r.name.as_deref() == Some(name)) {
                    *found = Some(LocalConstLookup::NonConst);
                }
                Self::scan_block_local_const(body, name, use_pos, self_sp, found);
            }
            Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
                scan_e(lhs, found);
                scan_e(rhs, found);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => scan_e(operand, found),
            Expr::Slice { base, lo, hi, .. } => {
                scan_e(base, found);
                lo.iter().chain(hi.iter()).for_each(|e| scan_e(e, found));
            }
            Expr::Call { callee, args, .. } => {
                scan_e(callee, found);
                args.iter().for_each(|a| scan_e(&a.value, found));
            }
            Expr::StructLit { fields, .. } => fields.iter().for_each(|f| scan_e(&f.value, found)),
            Expr::SliceLit { items, .. } => items.iter().for_each(|i| scan_e(i, found)),
            Expr::MapLit { entries, .. } => entries.iter().for_each(|(k, v)| {
                scan_e(k, found);
                scan_e(v, found);
            }),
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        scan_e(e, found);
                    }
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }
}

fn pos_le(a: Pos, b: Pos) -> bool {
    (a.line, a.col) <= (b.line, b.col)
}

fn span_contains(span: Span, pos: Pos) -> bool {
    pos_le(span.start, pos) && pos_le(pos, span.end)
}

fn stmt_span(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Local(l) => l.span,
        Stmt::Assign { span, .. } | Stmt::Expr { span, .. } | Stmt::Send { span, .. } | Stmt::If { span, .. }
        | Stmt::For { span, .. } | Stmt::Return { span, .. } | Stmt::Task { span, .. } | Stmt::Managed { span, .. }
        | Stmt::Match { span, .. } | Stmt::Defer { span, .. } | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. } | Stmt::Block { span, .. } | Stmt::Select { span, .. } => *span,
    }
}

fn is_composite_lit(e: &Expr) -> bool {
    matches!(
        unwrap_parens(e),
        Expr::SliceLit { .. } | Expr::MapLit { .. } | Expr::StructLit { .. }
    )
}

fn conversion_callee_name(callee: &Expr) -> Option<&str> {
    match unwrap_parens(callee) {
        Expr::Ident { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

/// Type names Hybrid spells that Go accepts as conversion targets in a
/// `const` initializer (`const x = int8(42)`).
pub(crate) fn is_conversion_type_name(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "string"
            | "byte"
            | "rune"
            | "char"
            | "int"
            | "int8"
            | "int16"
            | "int32"
            | "int64"
            | "uint"
            | "uint8"
            | "uint16"
            | "uint32"
            | "uint64"
            | "uintptr"
            | "float32"
            | "float64"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "isize"
            | "usize"
    )
}

fn span_hint(span: Span) -> String {
    format!("{}:{}", span.start.line, span.start.col)
}

#[cfg(test)]
mod tests {
    use super::is_conversion_type_name;

    #[test]
    fn conversion_type_names_cover_go_builtins() {
        assert!(is_conversion_type_name("int8"));
        assert!(is_conversion_type_name("byte"));
        assert!(!is_conversion_type_name("get_val"));
        assert!(!is_conversion_type_name("fmt"));
    }
}
