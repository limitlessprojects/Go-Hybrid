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
            if let hy_syntax::Decl::Const(c) = d {
                self.check_expr_const_zero_divisor(&crate::model::FnState::default(), &c.value);
            }
        }
    }

    pub(crate) fn check_stmt_const_zero_divisor(&mut self, st: &crate::model::FnState, stmt: &Stmt) {
        self.check_file_close_and_range_directions(stmt);
        self.zd_defined.clear();
        self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ false);
    }

    fn check_stmt_const_zero_divisor_inner(
        &mut self,
        st: &crate::model::FnState,
        stmt: &Stmt,
        deep: bool,
    ) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_const_zero_divisor(st, v);
                }
            }
            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                for t in targets {
                    self.check_place_const_zero_divisor(st, t);
                }
                for v in values {
                    self.check_expr_const_zero_divisor(st, v);
                }
                if matches!(op, AssignOp::DivAssign | AssignOp::ModAssign)
                    && values.len() == 1
                    && (is_int_literal_zero(&values[0])
                        || self.divisor_int_fold_is_zero(&values[0], st)
                        || targets.len() == 1
                            && self.divisor_untyped_float_zero(&values[0], st)
                            && self.place_is_known_int(st, &targets[0]))
                {
                    self.emit_const_zero_divisor(*span, assign_op_txt(*op));
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_const_zero_divisor(st, expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_const_zero_divisor(st, chan);
                self.check_expr_const_zero_divisor(st, value);
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
                        self.check_stmt_const_zero_divisor_inner(st, init, true);
                    }
                }
                self.check_expr_const_zero_divisor(st, cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_const_zero_divisor_inner(st, s, true);
                    }
                    if let Some(other) = otherwise {
                        self.check_stmt_const_zero_divisor_inner(st, other, true);
                    }
                }
            }
            Stmt::For { kind, body, .. } => {
                match kind {
                    ForKind::While { cond } => self.check_expr_const_zero_divisor(st, cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_const_zero_divisor(st, lo);
                        self.check_expr_const_zero_divisor(st, hi);
                    }
                    ForKind::Iterate { expr, .. } => self.check_expr_const_zero_divisor(st, expr),
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_const_zero_divisor_inner(st, s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_const_zero_divisor(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_const_zero_divisor(st, callee);
                for a in args {
                    self.check_expr_const_zero_divisor(st, &a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_const_zero_divisor_inner(st, s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_const_zero_divisor(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_const_zero_divisor(st, g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_const_zero_divisor_inner(st, s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_const_zero_divisor(st, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_const_zero_divisor_inner(st, s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value, .. } => {
                            self.check_expr_const_zero_divisor(st, chan);
                            self.check_expr_const_zero_divisor(st, value);
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_const_zero_divisor(st, chan)
                        }
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_const_zero_divisor_inner(st, s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_const_zero_divisor(&mut self, st: &crate::model::FnState, place: &Place) {
        match &place.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_const_zero_divisor(st, base);
                self.check_expr_const_zero_divisor(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_const_zero_divisor(st, base)
            }
        }
    }

    pub(crate) fn check_expr_const_zero_divisor(
        &mut self,
        st: &crate::model::FnState,
        expr: &Expr,
    ) {
        match expr {
            Expr::Binary { op, lhs, rhs, span } => {
                self.check_expr_const_zero_divisor(st, lhs);
                self.check_expr_const_zero_divisor(st, rhs);
                if matches!(op, BinOp::Div | BinOp::Mod) {
                    // Issue #758: the divisor predicate becomes const-map-
                    // aware (#998 pattern): a named constant — or any constant
                    // expression over them, incl. `len()` of a constant string
                    // and constant conversions — dividing by zero rejects like
                    // the literal twin. Shadow-checked against the live env;
                    // names defined inside a deep-walked closure body fail open
                    // (`self.zd_defined`). Runtime divisors stay out of scope.
                    if self.divisor_int_fold_is_zero(rhs, st)
                        || is_float_literal_zero(rhs) && is_const_expr(lhs)
                        || self.divisor_float_fold_is_zero(rhs, st)
                            && self.is_const_map_expr(lhs, st)
                        || self.divisor_untyped_float_zero(rhs, st)
                            && self.expr_is_known_int(st, lhs)
                    {
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
                self.check_expr_const_zero_divisor(st, operand)
            }
            Expr::Index { base, index, .. } => {
                self.check_expr_const_zero_divisor(st, base);
                self.check_expr_const_zero_divisor(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_const_zero_divisor(st, base);
                if let Some(lo) = lo {
                    self.check_expr_const_zero_divisor(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_const_zero_divisor(st, hi);
                }
            }
            Expr::Call { callee, args, .. } => {
                self.check_expr_const_zero_divisor(st, callee);
                for a in args {
                    self.check_expr_const_zero_divisor(st, &a.value);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_const_zero_divisor(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_const_zero_divisor(st, i);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_const_zero_divisor(st, k);
                    self.check_expr_const_zero_divisor(st, v);
                }
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_const_zero_divisor(st, e);
                    }
                }
            }
            Expr::FnLit { params, body, .. } => {
                // Closure bodies are not flattened into Pass B — deep-walk.
                // Their scopes differ from `st`: a closure-local shadow of a
                // file const is invisible to env_lookup, so resolution fails
                // open on every name the body defines (issue #758 guard).
                {
                    let mut defined = std::collections::HashSet::new();
                    for p in params {
                        defined.insert(p.name.clone());
                    }
                    for stmt in &body.stmts {
                        stmt_defined_names(stmt, &mut defined);
                    }
                    self.zd_defined = defined;
                }
                for stmt in &body.stmts {
                    self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ true);
                }
                self.zd_defined.clear();
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

fn is_int_literal_zero(expr: &Expr) -> bool {
    is_constant_zero(expr)
}

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

fn is_const_expr(expr: &Expr) -> bool {
    match unwrap_parens(expr) {
        Expr::Lit { .. } => true,
        Expr::Unary { operand, .. } => is_const_expr(operand),
        Expr::Binary { lhs, rhs, .. } => is_const_expr(lhs) && is_const_expr(rhs),
        _ => {
            const_eval_int(expr).is_some()
                || crate::const_overflow::const_float_value(expr).is_some()
        }
    }
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
    };
    if digits.is_empty() {
        return None;
    }
    i128::from_str_radix(digits, radix).ok()
}

#[allow(dead_code)]
fn int_lit_is_zero(raw: &str) -> bool {
    int_lit_value(raw) == Some(0)
}


// ---------------------------------------------------------------------------
// Issue #758: named/derived constant zero divisors (HYB-E6001).
// The #998 pattern: resolve through the file const map exactly like the
// declaration path; a bare syntactic fold sees a named const as NotConst.
// ---------------------------------------------------------------------------

/// Conservative fail-open guard: every name DEFINED anywhere inside a
/// deep-walked closure body. Closure bodies walk under the caller's scope
/// state (`check_stmt_const_zero_divisor_inner` deep mode), so a
/// closure-local shadow is invisible to `st`. Over-collecting is safe —
/// it only suppresses resolution (a missed detection, never a false
/// reject); the house rule is fail-open.
fn expr_defined_names(expr: &Expr, out: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::FnLit { params, body, .. } => {
            for p in params {
                out.insert(p.name.clone());
            }
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => expr_defined_names(operand, out),
        Expr::Index { base, index, .. } => {
            expr_defined_names(base, out);
            expr_defined_names(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_defined_names(base, out);
            if let Some(lo) = lo {
                expr_defined_names(lo, out);
            }
            if let Some(hi) = hi {
                expr_defined_names(hi, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            expr_defined_names(callee, out);
            for a in args {
                expr_defined_names(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_defined_names(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                expr_defined_names(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_defined_names(k, out);
                expr_defined_names(v, out);
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    expr_defined_names(e, out);
                }
            }
        }
        _ => {}
    }
}

fn stmt_defined_names(stmt: &Stmt, out: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Local(l) => {
            for n in &l.names {
                out.insert(n.clone());
            }
            for v in &l.values {
                expr_defined_names(v, out);
            }
        }
        Stmt::Assign { targets, values, .. } => {
            for t in targets {
                if let PlaceKind::Ident(n) = &t.kind {
                    out.insert(n.clone());
                }
            }
            for v in values {
                expr_defined_names(v, out);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Range { var, lo, hi, .. } => {
                    out.insert(var.clone());
                    expr_defined_names(lo, out);
                    expr_defined_names(hi, out);
                }
                ForKind::Iterate { vars, expr, .. } => {
                    for v in vars {
                        out.insert(v.clone());
                    }
                    expr_defined_names(expr, out);
                }
                ForKind::While { cond } => expr_defined_names(cond, out),
                ForKind::Infinite => {}
            }
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Match { subject, arms, .. } => {
            expr_defined_names(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    expr_defined_names(g, out);
                }
                for s in &arm.body.stmts {
                    stmt_defined_names(s, out);
                }
            }
        }
        Stmt::If {
            init, cond, then, otherwise, ..
        } => {
            if let Some(init) = init {
                stmt_defined_names(init, out);
            }
            expr_defined_names(cond, out);
            for s in &then.stmts {
                stmt_defined_names(s, out);
            }
            if let Some(other) = otherwise {
                stmt_defined_names(other, out);
            }
        }
        Stmt::Block { block, .. } => {
            for s in &block.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Managed { body, .. } => {
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Recv { names, chan, .. } => {
                        for n in names {
                            out.insert(n.clone());
                        }
                        expr_defined_names(chan, out);
                    }
                    SelectArmKind::Send { chan, value, .. } => {
                        expr_defined_names(chan, out);
                        expr_defined_names(value, out);
                    }
                    SelectArmKind::Default => {}
                }
                for s in &arm.body.stmts {
                    stmt_defined_names(s, out);
                }
            }
        }
        Stmt::Send { chan, value, .. } => {
            expr_defined_names(chan, out);
            expr_defined_names(value, out);
        }
        Stmt::Expr { expr, .. } => expr_defined_names(expr, out),
        Stmt::Return { values, .. } => {
            for v in values {
                expr_defined_names(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_defined_names(callee, out);
            for a in args {
                expr_defined_names(&a.value, out);
            }
        }
        Stmt::Defer { call, .. } => expr_defined_names(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

impl<'a> Checker<'a> {
    fn is_unshadowed_arith_callee(&self, name: &str, st: &crate::model::FnState) -> bool {
        st.env_lookup(name).is_none()
            && !self.zd_defined.contains(name)
            && !self.fns.contains_key(name)
            && !self.structs.contains_key(name)
            && !self.consts.contains_key(name)
    }

    fn is_arith_int_type_name(&self, name: &str) -> bool {
        is_arith_int_primitive(name)
            || self.type_aliases.get(name).is_some_and(|ty| {
                matches!(self.resolve_type_alias(ty), hy_syntax::TypeExpr::Named { name: t, .. }
                    if is_arith_int_primitive(&t) && !self.structs.contains_key(t.as_str()))
            })
    }

    fn is_arith_float_type_name(&self, name: &str) -> bool {
        matches!(name, "f32" | "f64" | "float32" | "float64")
            || self.type_aliases.get(name).is_some_and(|ty| {
                matches!(self.resolve_type_alias(ty), hy_syntax::TypeExpr::Named { name: t, .. }
                    if matches!(t.as_str(), "f32" | "f64" | "float32" | "float64") && !self.structs.contains_key(t.as_str()))
            })
    }

    fn zd_shadows_expr(&self, expr: &Expr) -> bool {
        match unwrap_parens(expr) {
            Expr::Ident { name, .. } => self.zd_defined.contains(name),
            Expr::Binary { lhs, rhs, .. } => self.zd_shadows_expr(lhs) || self.zd_shadows_expr(rhs),
            Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } => self.zd_shadows_expr(operand),
            _ => false,
        }
    }

    fn extract_complex_part<'e>(&'e self, e: &'e Expr, is_imag: bool, st: &crate::model::FnState, depth: u32) -> Option<&'e Expr> {
        if depth > 16 { return None; }
        match unwrap_parens(e) {
            Expr::Ident { name, .. } if st.env_lookup(name).is_none() && !self.zd_defined.contains(name) => {
                self.extract_complex_part(self.const_values.get(name.as_str())?, is_imag, st, depth + 1)
            }
            Expr::Call { callee, args, .. } if args.len() == 2 && !args[0].spread && !args[1].spread => {
                matches!(unwrap_parens(callee), Expr::Ident { name, .. }
                    if name == "complex" && self.is_unshadowed_arith_callee("complex", st) && !self.type_aliases.contains_key("complex"))
                    .then(|| &args[usize::from(is_imag)].value)
            }
            _ => None,
        }
    }

    fn divisor_int_fold(&self, e: &Expr, st: &crate::model::FnState, depth: u32) -> crate::const_overflow::ConstFold {
        use crate::const_overflow::{const_int_fold_with, ConstFold};
        if depth > 16 { return ConstFold::NotConst; }
        match unwrap_parens(e) {
            Expr::Ident { name, .. } => {
                if st.env_lookup(name).is_some() || self.zd_defined.contains(name) {
                    return ConstFold::NotConst;
                }
                self.const_values.get(name.as_str()).map_or(ConstFold::NotConst, |v| self.divisor_int_fold(v, st, depth + 1))
            }
            Expr::Unary { op: UnOp::Neg, operand, .. } => match self.divisor_int_fold(operand, st, depth + 1) {
                ConstFold::Val(v) => v.checked_neg().map_or(ConstFold::NotConst, ConstFold::Val),
                other => other,
            },
            Expr::Binary { op, lhs, rhs, .. } => crate::const_overflow::fold_bin(
                *op,
                self.divisor_int_fold(lhs, st, depth + 1),
                self.divisor_int_fold(rhs, st, depth + 1),
            ),
            Expr::Call { callee, args, span } if args.len() == 1 && !args[0].spread => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    let arg = &args[0].value;
                    if (name == "len" || name == "cap")
                        && self.is_unshadowed_arith_callee(name, st)
                        && !self.type_aliases.contains_key(name.as_str())
                        && !self.zd_shadows_expr(arg)
                    {
                        if name == "len" {
                            if let Some(n) = self.const_string_len_at(arg, span.start, 0) {
                                return ConstFold::Val(n);
                            }
                        }
                        if !expr_has_call_or_recv(arg) {
                            if let Some(n) = self.const_array_len_at(arg, span.start, 0) {
                                return ConstFold::Val(n);
                            }
                        }
                    }
                    if (name == "real" || name == "imag")
                        && self.is_unshadowed_arith_callee(name, st)
                        && !self.type_aliases.contains_key(name.as_str())
                    {
                        if let Some(part) = self.extract_complex_part(arg, name == "imag", st, depth + 1) {
                            return self.divisor_int_fold(part, st, depth + 1);
                        }
                    }
                    if self.is_unshadowed_arith_callee(name, st) && self.is_arith_int_type_name(name) {
                        if matches!(self.divisor_int_fold(arg, st, depth + 1), ConstFold::Val(0))
                            || self.divisor_float_fold_is_zero_at(arg, st, depth + 1)
                        {
                            return ConstFold::Val(0);
                        }
                    }
                }
                const_int_fold_with(e, &self.const_values, depth, Some(st))
            }
            _ => const_int_fold_with(e, &self.const_values, depth, Some(st)),
        }
    }

    fn divisor_int_fold_is_zero(&self, e: &Expr, st: &crate::model::FnState) -> bool {
        matches!(self.divisor_int_fold(e, st, 0), crate::const_overflow::ConstFold::Val(0))
    }

    fn divisor_float_fold_is_zero(&self, e: &Expr, st: &crate::model::FnState) -> bool {
        self.divisor_float_fold_is_zero_at(e, st, 0)
    }

    fn divisor_float_fold_is_zero_at(&self, e: &Expr, st: &crate::model::FnState, depth: u32) -> bool {
        if depth > 16 { return false; }
        let bare = unwrap_parens(e);
        match bare {
            Expr::Ident { name, .. } if st.env_lookup(name).is_none() && !self.zd_defined.contains(name) => {
                if let Some(v) = self.const_values.get(name.as_str()) {
                    return self.divisor_float_fold_is_zero_at(v, st, depth + 1);
                }
            }
            Expr::Unary { op: UnOp::Neg, operand, .. } => return self.divisor_float_fold_is_zero_at(operand, st, depth + 1),
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    let a = &args[0].value;
                    if self.is_unshadowed_arith_callee(name, st) && self.is_arith_float_type_name(name) {
                        return matches!(self.divisor_int_fold(a, st, depth + 1), crate::const_overflow::ConstFold::Val(0))
                            || self.divisor_float_fold_is_zero_at(a, st, depth + 1);
                    }
                    if (name == "real" || name == "imag")
                        && self.is_unshadowed_arith_callee(name, st)
                        && !self.type_aliases.contains_key(name.as_str())
                    {
                        if let Some(part) = self.extract_complex_part(a, name == "imag", st, depth + 1) {
                            return matches!(self.divisor_int_fold(part, st, depth + 1), crate::const_overflow::ConstFold::Val(0))
                                || self.divisor_float_fold_is_zero_at(part, st, depth + 1);
                        }
                    }
                }
            }
            _ => {}
        }
        matches!(crate::const_overflow::const_float_fold_with(bare, &self.const_values, depth, Some(st)), Some(f) if f == 0.0)
    }

    fn divisor_untyped_float_zero(&self, e: &Expr, st: &crate::model::FnState) -> bool {
        self.divisor_float_fold_is_zero(e, st) && !self.expr_has_explicit_float_type(e, st, 0)
    }

    fn expr_has_explicit_float_type(&self, e: &Expr, st: &crate::model::FnState, depth: u32) -> bool {
        if depth > 16 { return true; }
        match unwrap_parens(e) {
            Expr::Call { callee, .. } => matches!(unwrap_parens(callee), Expr::Ident { name, .. }
                if self.is_unshadowed_arith_callee(name, st) && (self.is_arith_float_type_name(name) || name == "real" || name == "imag")),
            Expr::Ident { name, .. } => self.file.decls.iter().any(|d| match d {
                hy_syntax::Decl::Const(c) if c.name == *name => {
                    c.ty.as_ref().is_some_and(|t| matches!(self.resolve_type_alias(t), hy_syntax::TypeExpr::Named { name: tn, .. } if self.is_arith_float_type_name(&tn)))
                        || self.expr_has_explicit_float_type(&c.value, st, depth + 1)
                }
                _ => false,
            }),
            Expr::Unary { operand, .. } => self.expr_has_explicit_float_type(operand, st, depth + 1),
            Expr::Binary { lhs, rhs, .. } => self.expr_has_explicit_float_type(lhs, st, depth + 1) || self.expr_has_explicit_float_type(rhs, st, depth + 1),
            _ => false,
        }
    }

    fn place_is_known_int(&self, st: &crate::model::FnState, place: &Place) -> bool {
        matches!(&place.kind, PlaceKind::Ident(name) if self.ident_is_known_int(st, name, place.span.start))
    }

    fn expr_is_known_int(&self, st: &crate::model::FnState, expr: &Expr) -> bool {
        if let Some(ty) = self.ty_of_expr(st, expr) {
            return matches!(self.resolve_type_alias(&ty), hy_syntax::TypeExpr::Named { name, .. } if self.is_arith_int_type_name(&name));
        }
        match unwrap_parens(expr) {
            Expr::Ident { name, span } => self.ident_is_known_int(st, name, span.start),
            Expr::Lit { kind: LitKind::Int(_), .. } => true,
            Expr::Unary { op: UnOp::Neg, operand, .. } => self.expr_is_known_int(st, operand),
            Expr::Binary { lhs, rhs, .. } => self.expr_is_known_int(st, lhs) && self.expr_is_known_int(st, rhs),
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => matches!(
                unwrap_parens(callee),
                Expr::Ident { name, .. } if self.is_unshadowed_arith_callee(name, st) && (name == "len" || name == "cap" || self.is_arith_int_type_name(name))
            ),
            _ => false,
        }
    }

    fn ident_is_known_int(&self, st: &crate::model::FnState, name: &str, pos: hy_diag::Pos) -> bool {
        if self.zd_defined.contains(name) { return false; }
        if let Some(ty) = self.lookup_binding(st, name).and_then(|b| b.ty.as_ref()) {
            return matches!(self.resolve_type_alias(ty), hy_syntax::TypeExpr::Named { name: tn, .. } if self.is_arith_int_type_name(&tn));
        }
        for decl in &self.file.decls {
            let hy_syntax::Decl::Fn(f) = decl else { continue };
            if pos_in_arith_span(f.span, pos) {
                let mut out = None;
                if let Some(body) = &f.body { self.scan_block_int_local(st, &body.stmts, name, pos, &mut out); }
                return out.unwrap_or(false);
            }
        }
        false
    }

    fn scan_block_int_local(&self, st: &crate::model::FnState, stmts: &[Stmt], target: &str, pos: hy_diag::Pos, out: &mut Option<bool>) {
        for s in stmts {
            if (pos.line, pos.col) <= (s.span().start.line, s.span().start.col) { break; }
            match s {
                Stmt::Local(l) => {
                    for (i, n) in l.names.iter().enumerate().filter(|(_, n)| n.as_str() == target) {
                        let _ = n;
                        *out = Some(if let Some(ty) = &l.ty {
                            matches!(self.resolve_type_alias(ty), hy_syntax::TypeExpr::Named { name: tn, .. } if self.is_arith_int_type_name(&tn))
                        } else {
                            l.values.len() == l.names.len() && self.expr_is_known_int(st, &l.values[i])
                        });
                    }
                }
                Stmt::For { kind, body, .. } if pos_in_arith_span(body.span, pos) => {
                    if matches!(kind, ForKind::Range { var, .. } if var == target) { *out = Some(true); }
                    self.scan_block_int_local(st, &body.stmts, target, pos, out);
                }
                Stmt::If { init, then, otherwise, .. } => {
                    if let Some(i) = init { self.scan_block_int_local(st, std::slice::from_ref(i.as_ref()), target, pos, out); }
                    if pos_in_arith_span(then.span, pos) {
                        self.scan_block_int_local(st, &then.stmts, target, pos, out);
                    } else if let Some(o) = otherwise {
                        self.scan_block_int_local(st, std::slice::from_ref(o.as_ref()), target, pos, out);
                    }
                }
                Stmt::Block { block: b, .. } | Stmt::Managed { body: b, .. } if pos_in_arith_span(b.span, pos) => {
                    self.scan_block_int_local(st, &b.stmts, target, pos, out);
                }
                _ => {}
            }
        }
    }

    fn is_const_map_expr(&self, e: &Expr, st: &crate::model::FnState) -> bool {
        is_const_expr(e)
            || !matches!(self.divisor_int_fold(e, st, 0), crate::const_overflow::ConstFold::NotConst)
            || crate::const_overflow::const_float_fold_with(e, &self.const_values, 0, Some(st)).is_some()
    }
}

fn pos_in_arith_span(span: Span, pos: hy_diag::Pos) -> bool {
    (span.start.line, span.start.col) <= (pos.line, pos.col) && (pos.line, pos.col) <= (span.end.line, span.end.col)
}

fn is_arith_int_primitive(name: &str) -> bool {
    crate::const_overflow::fixed_width_int_range(name).is_some()
        || matches!(name, "int" | "uint" | "uintptr" | "isize" | "usize")
}

fn expr_has_call_or_recv(expr: &Expr) -> bool {
    match unwrap_parens(expr) {
        Expr::Call { .. } | Expr::Receive { .. } => true,
        Expr::Unary { operand, .. } | Expr::AddrOf { operand, .. } | Expr::Move { operand, .. }
        | Expr::Try { operand, .. } | Expr::Field { base: operand, .. } => expr_has_call_or_recv(operand),
        Expr::Binary { lhs, rhs, .. } | Expr::Index { base: lhs, index: rhs, .. } => {
            expr_has_call_or_recv(lhs) || expr_has_call_or_recv(rhs)
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_has_call_or_recv(base) || lo.as_deref().is_some_and(expr_has_call_or_recv) || hi.as_deref().is_some_and(expr_has_call_or_recv)
        }
        Expr::SliceLit { items, .. } => items.iter().any(expr_has_call_or_recv),
        Expr::MapLit { entries, .. } => entries.iter().any(|(k, v)| expr_has_call_or_recv(k) || expr_has_call_or_recv(v)),
        Expr::StructLit { fields, .. } => fields.iter().any(|f| expr_has_call_or_recv(&f.value)),
        Expr::Interp { parts, .. } => parts.iter().any(|p| matches!(p, InterpPart::Expr(e) if expr_has_call_or_recv(e))),
        Expr::Lit { .. } | Expr::Ident { .. } | Expr::FnLit { .. } | Expr::Paren { .. } => false,
    }
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
