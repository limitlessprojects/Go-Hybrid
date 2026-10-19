//! arith.rs — constant arithmetic that would make emitted Go unbuildable.
//!
//! Issue #149: `x / 0` and `x % 0` with a literal `0` divisor pass
//! `hyb check` (0 diagnostics) and then fail `go vet` /
//! `go build` with `invalid operation: division by zero`. The accepted-
//! should-emit-buildable-Go contract requires a checker diagnostic
//! (HYB-E6001) for that narrow literal case. Non-constant zero divisors
//! stay out of scope — Go accepts them and panics at run time.

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, Expr, ForKind, InterpPart, LitKind, Place, PlaceKind, SelectArmKind,
    Stmt, UnOp,
};

impl<'a> Checker<'a> {
    /// Pass-B entry: walk the same expression surface as `walk::own_uses`.
    /// Nested block bodies are NOT recursed here — `walk::flatten` already
    /// lists those statements, and a second walk would duplicate diagnostics.
    pub(crate) fn check_file_const_zero_divisors(&mut self) {
        for d in &self.file.decls {
            match d {
                hy_syntax::Decl::Const(c) => {
                    if let Some(ty) = &c.ty {
                        self.check_type_const_zero_divisor(ty);
                    }
                    self.check_expr_const_zero_divisor(&c.value);
                }
                hy_syntax::Decl::TypeAlias(ta) => {
                    self.check_type_const_zero_divisor(&ta.ty);
                }
                hy_syntax::Decl::Struct(s) => {
                    for f in &s.fields {
                        self.check_type_const_zero_divisor(&f.ty);
                    }
                }
                hy_syntax::Decl::Fn(f) => {
                    if let Some(r) = &f.receiver {
                        self.check_type_const_zero_divisor(&r.ty);
                    }
                    for p in &f.params {
                        self.check_type_const_zero_divisor(&p.ty);
                    }
                    for res in &f.results {
                        self.check_type_const_zero_divisor(&res.ty);
                    }
                }
                _ => {}
            }
        }
    }

    fn check_type_const_zero_divisor(&mut self, ty: &hy_syntax::TypeExpr) {
        match ty {
            hy_syntax::TypeExpr::Array { len, elem, .. } => {
                if let Some(l) = len {
                    self.check_expr_const_zero_divisor(l);
                }
                self.check_type_const_zero_divisor(elem);
            }
            hy_syntax::TypeExpr::Pointer { to, .. }
            | hy_syntax::TypeExpr::Slice { elem: to, .. }
            | hy_syntax::TypeExpr::Chan { elem: to, .. }
            | hy_syntax::TypeExpr::Option { elem: to, .. } => {
                self.check_type_const_zero_divisor(to);
            }
            hy_syntax::TypeExpr::Map { key, value, .. } => {
                self.check_type_const_zero_divisor(key);
                self.check_type_const_zero_divisor(value);
            }
            hy_syntax::TypeExpr::Fn {
                params, results, ..
            } => {
                for p in params {
                    self.check_type_const_zero_divisor(&p.ty);
                }
                for r in results {
                    self.check_type_const_zero_divisor(r);
                }
            }
            hy_syntax::TypeExpr::Named { .. } => {}
        }
    }

    pub(crate) fn check_stmt_const_zero_divisor(&mut self, stmt: &Stmt) {
        self.check_file_close_and_range_directions(stmt);
        self.check_stmt_const_zero_divisor_inner(stmt, /*deep=*/ false);
    }

    fn check_stmt_const_zero_divisor_inner(&mut self, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                if let Some(ty) = &l.ty {
                    self.check_type_const_zero_divisor(ty);
                }
                for v in &l.values {
                    self.check_expr_const_zero_divisor(v);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.check_place_const_zero_divisor(t);
                }
                for v in values {
                    self.check_expr_const_zero_divisor(v);
                }
                if matches!(op, AssignOp::DivAssign | AssignOp::ModAssign)
                    && values.len() == 1
                    && self.is_int_const_zero(&values[0])
                {
                    self.emit_const_zero_divisor(*span, assign_op_txt(*op));
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_const_zero_divisor(expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_const_zero_divisor(chan);
                self.check_expr_const_zero_divisor(value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if deep {
                    if let Some(init) = init {
                        self.check_stmt_const_zero_divisor_inner(init, true);
                    }
                }
                self.check_expr_const_zero_divisor(cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_const_zero_divisor_inner(s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_const_zero_divisor_inner(other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_const_zero_divisor(cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_const_zero_divisor(lo);
                        self.check_expr_const_zero_divisor(hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_const_zero_divisor(expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_const_zero_divisor_inner(s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_const_zero_divisor(v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_const_zero_divisor(callee);
                for a in args {
                    self.check_expr_const_zero_divisor(&a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_const_zero_divisor_inner(s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_const_zero_divisor(subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_const_zero_divisor(g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_const_zero_divisor_inner(s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_const_zero_divisor(call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_const_zero_divisor_inner(s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_const_zero_divisor(chan);
                            self.check_expr_const_zero_divisor(value);
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_const_zero_divisor(chan)
                        }
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_const_zero_divisor_inner(s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_const_zero_divisor(&mut self, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_const_zero_divisor(base);
                self.check_expr_const_zero_divisor(index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_const_zero_divisor(base)
            }
        }
    }

    pub(crate) fn check_expr_const_zero_divisor(&mut self, expr: &Expr) {
        match expr {
            Expr::Binary { op, lhs, rhs, span } => {
                self.check_expr_const_zero_divisor(lhs);
                self.check_expr_const_zero_divisor(rhs);
                if matches!(op, BinOp::Div | BinOp::Mod) {
                    if self.is_int_const_zero(rhs) {
                        self.emit_const_zero_divisor(*span, bin_op_txt(*op));
                    } else if self.is_float_const_zero(rhs) && self.is_const_numeric_expr(lhs) {
                        self.emit_const_zero_divisor(*span, bin_op_txt(*op));
                    }
                }
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. }
            | Expr::Paren { inner: operand, .. }
            | Expr::Field { base: operand, .. }
            | Expr::Receive { chan: operand, .. } => {
                self.check_expr_const_zero_divisor(operand)
            }
            Expr::Index { base, index, .. } => {
                self.check_expr_const_zero_divisor(base);
                self.check_expr_const_zero_divisor(index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_const_zero_divisor(base);
                if let Some(lo) = lo {
                    self.check_expr_const_zero_divisor(lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_const_zero_divisor(hi);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_const_zero_divisor(callee);
                for a in args {
                    self.check_expr_const_zero_divisor(&a.value);
                }
            }
            Expr::StructLit { ty, fields, .. } => {
                if let Some(t) = ty {
                    self.check_type_const_zero_divisor(t);
                }
                for f in fields {
                    self.check_expr_const_zero_divisor(&f.value);
                }
            }
            Expr::SliceLit { elem, items, .. } => {
                if let Some(t) = elem {
                    self.check_type_const_zero_divisor(t);
                }
                for i in items {
                    self.check_expr_const_zero_divisor(i);
                }
            }
            Expr::MapLit {
                key,
                value,
                entries,
                ..
            } => {
                self.check_type_const_zero_divisor(key);
                self.check_type_const_zero_divisor(value);
                for (k, v) in entries {
                    self.check_expr_const_zero_divisor(k);
                    self.check_expr_const_zero_divisor(v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_const_zero_divisor(e);
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
                    self.check_type_const_zero_divisor(&p.ty);
                }
                for r in results {
                    self.check_type_const_zero_divisor(&r.ty);
                }
                // Closure bodies are not flattened into Pass B — deep-walk.
                for stmt in &body.stmts {
                    self.check_stmt_const_zero_divisor_inner(stmt, /*deep=*/ true);
                }
            }
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn emit_const_zero_divisor(&mut self, span: Span, op_txt: &str) {
        let msg = format!(
            "division by constant zero (`{}`): Go rejects this at compile time, so the emitted module would not build",
            op_txt
        );
        self.diags.push(
            Diagnostic::error("HYB-E6001", self.path.clone(), span, msg).with_notes(vec![
                "why: a literal `0` divisor is a Go compile error (`invalid operation: division by zero`); Hybrid's accepted programs must emit buildable Go (issue #149)".to_string(),
                "fix: use a non-zero constant, or bind the divisor to a variable if a run-time panic is the intended behaviour".to_string(),
            ]),
        );
    }

    fn is_int_const_zero(&self, expr: &Expr) -> bool {
        self.eval_const_int_at(expr, expr.span().start, 0) == Some(0)
    }

    fn is_float_const_zero(&self, expr: &Expr) -> bool {
        self.eval_const_float_at(expr, expr.span().start, 0)
            .is_some_and(|f| f == 0.0)
    }

    fn is_const_numeric_expr(&self, expr: &Expr) -> bool {
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: LitKind::Int(_) | LitKind::Float(_),
                ..
            } => true,
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self.is_const_numeric_expr(operand),
            Expr::Binary { lhs, rhs, .. } => {
                self.is_const_numeric_expr(lhs) && self.is_const_numeric_expr(rhs)
            }
            _ => {
                let pos = expr.span().start;
                self.eval_const_int_at(expr, pos, 0).is_some()
                    || self.eval_const_float_at(expr, pos, 0).is_some()
            }
        }
    }

    fn is_float_type_annotation(&self, ty: &hy_syntax::TypeExpr) -> bool {
        match self.resolve_type_alias(ty) {
            hy_syntax::TypeExpr::Named { name, .. } => {
                matches!(name.as_str(), "f32" | "f64" | "float32" | "float64")
            }
            _ => false,
        }
    }

    fn is_int_type_annotation(&self, ty: &hy_syntax::TypeExpr) -> bool {
        match self.resolve_type_alias(ty) {
            hy_syntax::TypeExpr::Named { name, .. } => matches!(
                name.as_str(),
                "int"
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
                    | "byte"
                    | "rune"
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
            ),
            _ => false,
        }
    }

    fn is_float_conversion_name(&self, name: &str) -> bool {
        if matches!(name, "f32" | "f64" | "float32" | "float64") {
            return true;
        }
        if let Some(ty) = self.type_aliases.get(name) {
            return self.is_float_type_annotation(ty);
        }
        false
    }

    fn is_int_conversion_name_arith(&self, name: &str) -> bool {
        if matches!(
            name,
            "int"
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
                | "byte"
                | "rune"
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
        ) {
            return true;
        }
        if let Some(ty) = self.type_aliases.get(name) {
            return self.is_int_type_annotation(ty);
        }
        false
    }

    fn eval_const_int_at(
        &self,
        expr: &Expr,
        use_pos: hy_diag::Pos,
        depth: usize,
    ) -> Option<i128> {
        if depth > 24 {
            return None;
        }
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: LitKind::Int(raw),
                ..
            } => int_lit_value(raw),
            Expr::Ident { name, span } => {
                match self.resolve_lexical_binding(name, span.start) {
                    Some(LexicalBinding::NonConst) => None,
                    Some(LexicalBinding::LocalConst { ty, val, decl_pos }) => {
                        if ty.as_ref().is_some_and(|t| self.is_float_type_annotation(t)) {
                            return None;
                        }
                        if (decl_pos.line, decl_pos.col) == (use_pos.line, use_pos.col) {
                            return None;
                        }
                        self.eval_const_int_at(val, decl_pos, depth + 1)
                    }
                    None => {
                        let d = self.file.decls.iter().find_map(|d| match d {
                            hy_syntax::Decl::Const(c) if c.name == *name => Some(c),
                            _ => None,
                        })?;
                        if d.ty
                            .as_ref()
                            .is_some_and(|t| self.is_float_type_annotation(t))
                        {
                            return None;
                        }
                        self.eval_const_int_at(&d.value, d.span.start, depth + 1)
                    }
                }
            }
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self
                .eval_const_int_at(operand, use_pos, depth)
                .and_then(|v| v.checked_neg()),
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.eval_const_int_at(lhs, use_pos, depth)?;
                let r = self.eval_const_int_at(rhs, use_pos, depth)?;
                match op {
                    BinOp::Add => l.checked_add(r),
                    BinOp::Sub => l.checked_sub(r),
                    BinOp::Mul => l.checked_mul(r),
                    BinOp::Div if r != 0 => l.checked_div(r),
                    BinOp::Mod if r != 0 => l.checked_rem(r),
                    BinOp::BitAnd => Some(l & r),
                    BinOp::BitOr => Some(l | r),
                    BinOp::BitXor => Some(l ^ r),
                    BinOp::Shl if (0..128).contains(&r) => l.checked_shl(r as u32),
                    BinOp::Shr if (0..128).contains(&r) => l.checked_shr(r as u32),
                    _ => None,
                }
            }
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                let Expr::Ident {
                    name: fn_name,
                    span: fn_span,
                } = unwrap_parens(callee)
                else {
                    return None;
                };
                if self.fns.contains_key(fn_name.as_str())
                    || self.resolve_lexical_binding(fn_name, fn_span.start).is_some()
                {
                    return None;
                }
                if fn_name == "len" {
                    return self.const_string_len_at(&args[0].value, use_pos, depth);
                }
                if self.is_int_conversion_name_arith(fn_name) {
                    if let Some(v) = self.eval_const_int_at(&args[0].value, use_pos, depth) {
                        return Some(v);
                    }
                    if let Some(f) = self.eval_const_float_at(&args[0].value, use_pos, depth) {
                        if f.is_finite()
                            && f.fract() == 0.0
                            && f >= (i64::MIN as f64)
                            && f <= (i64::MAX as f64)
                        {
                            return Some(f as i128);
                        }
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn eval_const_float_at(
        &self,
        expr: &Expr,
        use_pos: hy_diag::Pos,
        depth: usize,
    ) -> Option<f64> {
        if depth > 24 {
            return None;
        }
        match unwrap_parens(expr) {
            Expr::Lit {
                kind: LitKind::Float(raw),
                ..
            } => {
                if float_lit_is_zero(raw) {
                    Some(0.0)
                } else {
                    crate::const_overflow::const_float_value(expr)
                }
            }
            Expr::Ident { name, span } => {
                match self.resolve_lexical_binding(name, span.start) {
                    Some(LexicalBinding::NonConst) => None,
                    Some(LexicalBinding::LocalConst { ty, val, decl_pos }) => {
                        if ty.as_ref().is_some_and(|t| self.is_int_type_annotation(t)) {
                            return None;
                        }
                        if (decl_pos.line, decl_pos.col) == (use_pos.line, use_pos.col) {
                            return None;
                        }
                        if let Some(f) = self.eval_const_float_at(val, decl_pos, depth + 1) {
                            return Some(f);
                        }
                        if ty.as_ref().is_some_and(|t| self.is_float_type_annotation(t)) {
                            return self
                                .eval_const_int_at(val, decl_pos, depth + 1)
                                .map(|i| i as f64);
                        }
                        None
                    }
                    None => {
                        let d = self.file.decls.iter().find_map(|d| match d {
                            hy_syntax::Decl::Const(c) if c.name == *name => Some(c),
                            _ => None,
                        })?;
                        if d.ty
                            .as_ref()
                            .is_some_and(|t| self.is_int_type_annotation(t))
                        {
                            return None;
                        }
                        if let Some(f) = self.eval_const_float_at(&d.value, d.span.start, depth + 1)
                        {
                            return Some(f);
                        }
                        if d.ty
                            .as_ref()
                            .is_some_and(|t| self.is_float_type_annotation(t))
                        {
                            return self
                                .eval_const_int_at(&d.value, d.span.start, depth + 1)
                                .map(|i| i as f64);
                        }
                        None
                    }
                }
            }
            Expr::Unary {
                op: UnOp::Neg,
                operand,
                ..
            } => self.eval_const_float_at(operand, use_pos, depth).map(|v| -v),
            Expr::Binary { op, lhs, rhs, .. } => {
                let lf = self.eval_const_float_at(lhs, use_pos, depth);
                let rf = self.eval_const_float_at(rhs, use_pos, depth);
                if lf.is_none() && rf.is_none() {
                    return None;
                }
                let l = lf.or_else(|| {
                    self.eval_const_int_at(lhs, use_pos, depth)
                        .map(|i| i as f64)
                })?;
                let r = rf.or_else(|| {
                    self.eval_const_int_at(rhs, use_pos, depth)
                        .map(|i| i as f64)
                })?;
                match op {
                    BinOp::Add => Some(l + r),
                    BinOp::Sub => Some(l - r),
                    BinOp::Mul => Some(l * r),
                    BinOp::Div if r != 0.0 => Some(l / r),
                    _ => None,
                }
            }
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                let Expr::Ident {
                    name: fn_name,
                    span: fn_span,
                } = unwrap_parens(callee)
                else {
                    return None;
                };
                if self.fns.contains_key(fn_name.as_str())
                    || self.resolve_lexical_binding(fn_name, fn_span.start).is_some()
                {
                    return None;
                }
                if self.is_float_conversion_name(fn_name) {
                    if let Some(f) = self.eval_const_float_at(&args[0].value, use_pos, depth) {
                        return Some(f);
                    }
                    if let Some(i) = self.eval_const_int_at(&args[0].value, use_pos, depth) {
                        return Some(i as f64);
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn resolve_lexical_binding(
        &self,
        name: &str,
        use_pos: hy_diag::Pos,
    ) -> Option<LexicalBinding<'a>> {
        for decl in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = decl {
                if pos_in_span(use_pos, f.span) {
                    let mut state = None;
                    if let Some(r) = &f.receiver {
                        if r.name == name {
                            state = Some(LexicalBinding::NonConst);
                        }
                    }
                    if f.params.iter().any(|p| p.name == name) {
                        state = Some(LexicalBinding::NonConst);
                    }
                    if f.results
                        .iter()
                        .any(|r| r.name.as_deref() == Some(name))
                    {
                        state = Some(LexicalBinding::NonConst);
                    }
                    if let Some(body) = &f.body {
                        scan_block_binding(body, name, use_pos, &mut state);
                    }
                    return state;
                }
            } else if let hy_syntax::Decl::Const(c) = decl {
                if pos_in_span(use_pos, c.value.span()) {
                    let mut state = None;
                    scan_expr_binding(&c.value, name, use_pos, &mut state);
                    return state;
                }
            }
        }
        None
    }
}

enum LexicalBinding<'a> {
    NonConst,
    LocalConst {
        ty: &'a Option<hy_syntax::TypeExpr>,
        val: &'a Expr,
        decl_pos: hy_diag::Pos,
    },
}

fn pos_le(a: hy_diag::Pos, b: hy_diag::Pos) -> bool {
    (a.line, a.col) <= (b.line, b.col)
}

fn pos_in_span(p: hy_diag::Pos, s: Span) -> bool {
    pos_le(s.start, p) && pos_le(p, s.end)
}

fn scan_block_binding<'a>(
    block: &'a hy_syntax::Block,
    name: &str,
    use_pos: hy_diag::Pos,
    state: &mut Option<LexicalBinding<'a>>,
) {
    if !pos_in_span(use_pos, block.span) {
        return;
    }
    for stmt in &block.stmts {
        let sp = stmt_span_arith(stmt);
        if pos_in_span(use_pos, sp) {
            scan_stmt_binding(stmt, name, use_pos, state);
            return;
        }
        if pos_le(sp.end, use_pos) {
            apply_stmt_binding(stmt, name, state);
        }
    }
}

fn apply_stmt_binding<'a>(
    stmt: &'a Stmt,
    name: &str,
    state: &mut Option<LexicalBinding<'a>>,
) {
    if let Stmt::Local(l) = stmt {
        if let Some(idx) = l.names.iter().position(|n| n == name) {
            if matches!(l.kind, hy_syntax::LocalKind::Const) {
                if let Some(val) = l.values.get(idx) {
                    *state = Some(LexicalBinding::LocalConst {
                        ty: &l.ty,
                        val,
                        decl_pos: l.span.start,
                    });
                } else {
                    *state = Some(LexicalBinding::NonConst);
                }
            } else {
                *state = Some(LexicalBinding::NonConst);
            }
        }
    }
}

fn stmt_span_arith(stmt: &Stmt) -> Span {
    match stmt {
        Stmt::Local(l) => l.span,
        Stmt::Assign { span, .. }
        | Stmt::Expr { span, .. }
        | Stmt::Send { span, .. }
        | Stmt::If { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Task { span, .. }
        | Stmt::Managed { span, .. }
        | Stmt::Match { span, .. }
        | Stmt::Defer { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. }
        | Stmt::Block { span, .. }
        | Stmt::Select { span, .. } => *span,
    }
}

fn scan_stmt_binding<'a>(
    stmt: &'a Stmt,
    name: &str,
    use_pos: hy_diag::Pos,
    state: &mut Option<LexicalBinding<'a>>,
) {
    match stmt {
        Stmt::Local(l) => {
            for v in &l.values {
                if pos_in_span(use_pos, v.span()) {
                    scan_expr_binding(v, name, use_pos, state);
                    return;
                }
            }
        }
        Stmt::Assign { values, .. } | Stmt::Return { values, .. } => {
            for v in values {
                if pos_in_span(use_pos, v.span()) {
                    scan_expr_binding(v, name, use_pos, state);
                    return;
                }
            }
        }
        Stmt::Expr { expr, .. } | Stmt::Defer { call: expr, .. } => {
            if pos_in_span(use_pos, expr.span()) {
                scan_expr_binding(expr, name, use_pos, state);
            }
        }
        Stmt::Send { chan, value, .. } => {
            if pos_in_span(use_pos, chan.span()) {
                scan_expr_binding(chan, name, use_pos, state);
            } else if pos_in_span(use_pos, value.span()) {
                scan_expr_binding(value, name, use_pos, state);
            }
        }
        Stmt::Task { callee, args, .. } => {
            if pos_in_span(use_pos, callee.span()) {
                scan_expr_binding(callee, name, use_pos, state);
                return;
            }
            for a in args {
                if pos_in_span(use_pos, a.value.span()) {
                    scan_expr_binding(&a.value, name, use_pos, state);
                    return;
                }
            }
        }
        Stmt::If {
            init,
            cond,
            then,
            otherwise,
            ..
        } => {
            if let Some(i) = init {
                let isp = stmt_span_arith(i);
                if pos_in_span(use_pos, isp) {
                    scan_stmt_binding(i, name, use_pos, state);
                    return;
                }
                if pos_le(isp.end, use_pos) {
                    apply_stmt_binding(i, name, state);
                }
            }
            if pos_in_span(use_pos, cond.span()) {
                scan_expr_binding(cond, name, use_pos, state);
            } else if pos_in_span(use_pos, then.span) {
                scan_block_binding(then, name, use_pos, state);
            } else if let Some(other) = otherwise {
                if pos_in_span(use_pos, stmt_span_arith(other)) {
                    scan_stmt_binding(other, name, use_pos, state);
                }
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::While { cond } if pos_in_span(use_pos, cond.span()) => {
                    scan_expr_binding(cond, name, use_pos, state);
                    return;
                }
                ForKind::Range { lo, hi, .. } => {
                    if pos_in_span(use_pos, lo.span()) {
                        scan_expr_binding(lo, name, use_pos, state);
                        return;
                    }
                    if pos_in_span(use_pos, hi.span()) {
                        scan_expr_binding(hi, name, use_pos, state);
                        return;
                    }
                }
                ForKind::Iterate { expr, .. } if pos_in_span(use_pos, expr.span()) => {
                    scan_expr_binding(expr, name, use_pos, state);
                    return;
                }
                _ => {}
            }
            if pos_in_span(use_pos, body.span) {
                match kind {
                    ForKind::Range { var, .. } if var == name => {
                        *state = Some(LexicalBinding::NonConst);
                    }
                    ForKind::Iterate { vars, .. } if vars.iter().any(|v| v == name) => {
                        *state = Some(LexicalBinding::NonConst);
                    }
                    _ => {}
                }
                scan_block_binding(body, name, use_pos, state);
            }
        }
        Stmt::Managed { body, .. } | Stmt::Block { block: body, .. } => {
            scan_block_binding(body, name, use_pos, state);
        }
        Stmt::Match { subject, arms, .. } => {
            if pos_in_span(use_pos, subject.span()) {
                scan_expr_binding(subject, name, use_pos, state);
                return;
            }
            for arm in arms {
                let in_guard = arm
                    .guard
                    .as_ref()
                    .is_some_and(|g| pos_in_span(use_pos, g.span()));
                if in_guard || pos_in_span(use_pos, arm.body.span) {
                    if let hy_syntax::MatchPattern::Path {
                        binding: Some(b), ..
                    } = &arm.pattern
                    {
                        if b == name {
                            *state = Some(LexicalBinding::NonConst);
                        }
                    }
                    if let Some(g) = &arm.guard {
                        if pos_in_span(use_pos, g.span()) {
                            scan_expr_binding(g, name, use_pos, state);
                            return;
                        }
                    }
                    scan_block_binding(&arm.body, name, use_pos, state);
                    return;
                }
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Send { chan, value, .. } => {
                        if pos_in_span(use_pos, chan.span()) {
                            scan_expr_binding(chan, name, use_pos, state);
                            return;
                        }
                        if pos_in_span(use_pos, value.span()) {
                            scan_expr_binding(value, name, use_pos, state);
                            return;
                        }
                    }
                    SelectArmKind::Recv { chan, .. } => {
                        if pos_in_span(use_pos, chan.span()) {
                            scan_expr_binding(chan, name, use_pos, state);
                            return;
                        }
                    }
                    SelectArmKind::Default => {}
                }
                if pos_in_span(use_pos, arm.body.span) {
                    if let SelectArmKind::Recv { names, .. } = &arm.kind {
                        if names.iter().any(|n| n == name) {
                            *state = Some(LexicalBinding::NonConst);
                        }
                    }
                    scan_block_binding(&arm.body, name, use_pos, state);
                    return;
                }
            }
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

fn scan_expr_binding<'a>(
    expr: &'a Expr,
    name: &str,
    use_pos: hy_diag::Pos,
    state: &mut Option<LexicalBinding<'a>>,
) {
    match expr {
        Expr::FnLit {
            params,
            results,
            body,
            ..
        } => {
            if pos_in_span(use_pos, body.span) {
                if params.iter().any(|p| p.name == name)
                    || results.iter().any(|r| r.name.as_deref() == Some(name))
                {
                    *state = Some(LexicalBinding::NonConst);
                }
                scan_block_binding(body, name, use_pos, state);
            }
        }
        Expr::Binary { lhs, rhs, .. } => {
            if pos_in_span(use_pos, lhs.span()) {
                scan_expr_binding(lhs, name, use_pos, state);
            } else if pos_in_span(use_pos, rhs.span()) {
                scan_expr_binding(rhs, name, use_pos, state);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => {
            if pos_in_span(use_pos, operand.span()) {
                scan_expr_binding(operand, name, use_pos, state);
            }
        }
        Expr::Index { base, index, .. } => {
            if pos_in_span(use_pos, base.span()) {
                scan_expr_binding(base, name, use_pos, state);
            } else if pos_in_span(use_pos, index.span()) {
                scan_expr_binding(index, name, use_pos, state);
            }
        }
        Expr::Slice { base, lo, hi, .. } => {
            if pos_in_span(use_pos, base.span()) {
                scan_expr_binding(base, name, use_pos, state);
            } else if let Some(l) = lo.as_ref().filter(|l| pos_in_span(use_pos, l.span())) {
                scan_expr_binding(l, name, use_pos, state);
            } else if let Some(h) = hi.as_ref().filter(|h| pos_in_span(use_pos, h.span())) {
                scan_expr_binding(h, name, use_pos, state);
            }
        }
        Expr::Call { callee, args, .. } => {
            if pos_in_span(use_pos, callee.span()) {
                scan_expr_binding(callee, name, use_pos, state);
                return;
            }
            for a in args {
                if pos_in_span(use_pos, a.value.span()) {
                    scan_expr_binding(&a.value, name, use_pos, state);
                    return;
                }
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                if pos_in_span(use_pos, f.value.span()) {
                    scan_expr_binding(&f.value, name, use_pos, state);
                    return;
                }
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                if pos_in_span(use_pos, i.span()) {
                    scan_expr_binding(i, name, use_pos, state);
                    return;
                }
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                if pos_in_span(use_pos, k.span()) {
                    scan_expr_binding(k, name, use_pos, state);
                    return;
                }
                if pos_in_span(use_pos, v.span()) {
                    scan_expr_binding(v, name, use_pos, state);
                    return;
                }
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    if pos_in_span(use_pos, e.span()) {
                        scan_expr_binding(e, name, use_pos, state);
                        return;
                    }
                }
            }
        }
        Expr::Ident { .. } | Expr::Lit { .. } => {}
    }
}

fn bin_op_txt(op: BinOp) -> &'static str {
    match op {
        BinOp::Div => "/",
        BinOp::Mod => "%",
        _ => "?",
    }
}

fn assign_op_txt(op: AssignOp) -> &'static str {
    match op {
        AssignOp::DivAssign => "/=",
        AssignOp::ModAssign => "%=",
        _ => "?",
    }
}

pub(crate) fn is_constant_zero(expr: &Expr) -> bool {
    const_eval_int(expr) == Some(0)
}

#[allow(dead_code)]
fn is_int_literal_zero(expr: &Expr) -> bool {
    is_constant_zero(expr)
}

#[allow(dead_code)]
pub(crate) fn is_float_literal_zero(expr: &Expr) -> bool {
    match unwrap_parens(expr) {
        Expr::Lit {
            kind: LitKind::Float(raw),
            ..
        } => float_lit_is_zero(raw),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => is_float_literal_zero(operand),
        _ => false,
    }
}

pub(crate) fn float_lit_is_zero(raw: &str) -> bool {
    let s: String = raw.chars().filter(|c| *c != '_').collect();
    if s.is_empty() {
        return false;
    }
    if s.starts_with("0x") || s.starts_with("0X") {
        let without_prefix = &s[2..];
        let mantissa = without_prefix.split(['p', 'P']).next().unwrap_or(without_prefix);
        let mantissa_clean: String = mantissa.chars().filter(|c| *c != '.').collect();
        return !mantissa_clean.is_empty() && mantissa_clean.chars().all(|c| c == '0');
    }
    let mantissa = s.split(['e', 'E']).next().unwrap_or(&s);
    let mantissa_clean: String = mantissa.chars().filter(|c| *c != '.').collect();
    !mantissa_clean.is_empty() && mantissa_clean.chars().all(|c| c == '0')
}

pub(crate) fn const_eval_int(expr: &Expr) -> Option<i128> {
    match unwrap_parens(expr) {
        Expr::Lit {
            kind: LitKind::Int(raw),
            ..
        } => int_lit_value(raw),
        Expr::Unary { op, operand, .. } => {
            let val = const_eval_int(operand)?;
            match op {
                UnOp::Neg => val.checked_neg(),
                _ => None,
            }
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_eval_int(lhs)?;
            let r = const_eval_int(rhs)?;
            match op {
                BinOp::Add => l.checked_add(r),
                BinOp::Sub => l.checked_sub(r),
                BinOp::Mul => l.checked_mul(r),
                BinOp::Div => {
                    if r != 0 {
                        l.checked_div(r)
                    } else {
                        None
                    }
                }
                BinOp::Mod => {
                    if r != 0 {
                        l.checked_rem(r)
                    } else {
                        None
                    }
                }
                BinOp::BitAnd => Some(l & r),
                BinOp::BitOr => Some(l | r),
                BinOp::BitXor => Some(l ^ r),
                BinOp::Shl => {
                    if r >= 0 && r < 128 {
                        l.checked_shl(r as u32)
                    } else {
                        None
                    }
                }
                BinOp::Shr => {
                    if r >= 0 && r < 128 {
                        l.checked_shr(r as u32)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

pub(crate) fn int_lit_value(raw: &str) -> Option<i128> {
    let s: String = raw.chars().filter(|c| *c != '_').collect();
    if s.is_empty() {
        return None;
    }
    let (radix, digits) = if let Some(rest) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X"))
    {
        (16, rest)
    } else if let Some(rest) = s.strip_prefix("0o").or_else(|| s.strip_prefix("0O")) {
        (8, rest)
    } else if let Some(rest) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        (2, rest)
    } else if s.len() > 1 && s.starts_with('0') {
        if s.chars().any(|c| c == '8' || c == '9') {
            return None;
        }
        (8, s.as_str())
    } else {
        (10, s.as_str())
    }
    ;
    if digits.is_empty() {
        return None;
    }
    i128::from_str_radix(digits, radix).ok()
}

#[allow(dead_code)]
fn int_lit_is_zero(raw: &str) -> bool {
    int_lit_value(raw) == Some(0)
}

#[cfg(test)]
mod lit_zero_tests {
    use super::{int_lit_is_zero, is_int_literal_zero};
    use hy_diag::{Pos, Span};
    use hy_syntax::{Expr, LitKind, UnOp};

    fn make_int(s: &str) -> Expr {
        Expr::Lit {
            kind: LitKind::Int(s.to_string()),
            span: Span::new(Pos::new(1, 1), Pos::new(1, 1)),
        }
    }

    fn make_neg(inner: Expr) -> Expr {
        Expr::Unary {
            op: UnOp::Neg,
            operand: Box::new(inner),
            span: Span::new(Pos::new(1, 1), Pos::new(1, 1)),
        }
    }

    #[test]
    fn recognizes_zero_spellings() {
        for s in ["0", "00", "0_0", "0x0", "0X00", "0o0", "0b0", "0b000"] {
            assert!(int_lit_is_zero(s), "{s}");
            assert!(is_int_literal_zero(&make_int(s)), "{s}");
            assert!(is_int_literal_zero(&make_neg(make_int(s))), "-{s}");
            assert!(is_int_literal_zero(&make_neg(make_neg(make_int(s)))), "--{s}");
        }
    }

    #[test]
    fn rejects_nonzero() {
        for s in ["1", "10", "0x1", "0b1", "0o7", "00_1"] {
            assert!(!int_lit_is_zero(s), "{s}");
            assert!(!is_int_literal_zero(&make_int(s)), "{s}");
            assert!(!is_int_literal_zero(&make_neg(make_int(s))), "-{s}");
        }
    }
}
