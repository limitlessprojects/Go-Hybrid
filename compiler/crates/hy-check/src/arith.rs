//! arith.rs — constant arithmetic that would make emitted Go unbuildable.
//!
//! Issue #149: `x / 0` and `x % 0` with a literal `0` divisor pass
//! `hyb check` (0 diagnostics) and then fail `go vet` /
//! `go build` with `invalid operation: division by zero`. The accepted-
//! should-emit-buildable-Go contract requires a checker diagnostic
//! (HYB-E6001) for that narrow literal case. Non-constant zero divisors
//! stay out of scope — Go accepts them and panics at run time.
//!
//! Issue #2426: Go folds more divisors to a constant zero than the literal
//! spellings above, and each one is still `invalid operation: division by
//! zero` at compile time. The folds below now cover the five gaps the report
//! names: an integer conversion of a constant float zero (`int(0.0)`,
//! `i64(1.5 - 1.5)`, `const fz = 0.0; int(fz)`, aliases of integer types),
//! `len()` of concatenated constant strings, `len`/`cap` of `[0]T` arrays,
//! `real`/`imag` selected from a constant `complex(re, im)`, and an untyped
//! `0.0` divisor on an integer dividend (where Go converts the untyped
//! constant to the dividend's type, making the divisor the integer zero).

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{
    AssignOp, BinOp, Expr, ForKind, InterpPart, LitKind, Place, PlaceKind, SelectArmKind, Stmt,
    TypeExpr, UnOp,
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
                        || divisor_int_fold_is_zero(
                            &values[0],
                            &self.const_values,
                            st,
                            &self.zd_defined,
                            &self.type_aliases,
                        )
                        // `x /= 0.0` on an integer target: Go converts the untyped
                        // constant to the target's type, so the divisor is the
                        // integer zero (#2426).
                        || (is_float_literal_zero(&values[0])
                            && targets
                                .first()
                                .map(|t| self.int_typed_place_target(t, st))
                                .unwrap_or(false)))
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
                    if divisor_int_fold_is_zero(
                        rhs,
                        &self.const_values,
                        st,
                        &self.zd_defined,
                        &self.type_aliases,
                    ) || is_float_literal_zero(rhs) && is_const_expr(lhs)
                        // `x / 0.0` on an integer dividend: the untyped constant
                        // converts to the dividend's type, so the divisor is the
                        // integer zero. A `float64` dividend keeps IEEE-754
                        // division at run time (#2426).
                        || is_float_literal_zero(rhs) && self.int_typed_dividend_at(lhs, st, 0)
                        || divisor_float_fold_is_zero(
                            rhs,
                            &self.const_values,
                            st,
                            &self.zd_defined,
                            &self.type_aliases,
                        ) && is_const_map_expr(
                            lhs,
                            &self.const_values,
                            st,
                            &self.zd_defined,
                            &self.type_aliases,
                        )
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

/// Byte length of a constant string expression: literals, named const strings,
/// and concatenations of them. Go folds the concatenation first and the length
/// second, so `len("" + "")` and chained string constants are compile-time `0`
/// (#2426).
fn const_str_len_bytes(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Str(s) | LitKind::RawStr(s),
            ..
        } => Some(s.len() as i128),
        Expr::Ident { name, .. } if !defined.contains(name) => {
            let v = consts.get(name.as_str())?;
            const_str_len_bytes(v, consts, defined, depth + 1)
        }
        Expr::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
            ..
        } => {
            let l = const_str_len_bytes(lhs, consts, defined, depth + 1)?;
            let r = const_str_len_bytes(rhs, consts, defined, depth + 1)?;
            l.checked_add(r)
        }
        _ => None,
    }
}

/// `len("abc")` — Go evaluates the length of a constant string at compile
/// time (spec: constant expression). Named const strings resolve through
/// the same map, and so do constant concatenations of them. The length is
/// the BYTE length (Go `len` on a string).
fn const_len_str_value(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if name == "len" && args.len() == 1 && !args[0].spread {
                return const_str_len_bytes(&args[0].value, consts, defined, depth + 1);
            }
        }
    }
    None
}

/// The #758 divisor fold: literals, named consts (shadow-checked against
/// the live env AND the deep-walk tree's defines), binary arithmetic over
/// folded leaves, and `len()` of constant strings. Everything else —
/// including fixed-width int conversions of constants — delegates to the
/// shared `const_int_fold_with` (#725 machinery).
fn divisor_int_fold(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
    depth: u32,
) -> crate::const_overflow::ConstFold {
    use crate::const_overflow::{const_int_fold_with, ConstFold};
    if depth > 16 {
        return ConstFold::NotConst;
    }
    let bare = unwrap_parens(e);
    // `len("abc")`, and now `len("" + "")` / `len(<named const string>)` (#2426).
    if let Some(n) = const_len_str_value(bare, consts, defined, depth) {
        return ConstFold::Val(n);
    }
    // `len([0]int{})`, `cap([0]int{})`, `len(a)` for `var a [0]int` (#2426): Go
    // folds `len`/`cap` of a fixed-size array to its constant length, so a
    // zero-length array is a constant zero divisor.
    if let Some(n) = const_len_cap_value(bare, st, defined, aliases, depth) {
        return ConstFold::Val(n);
    }
    match bare {
        Expr::Ident { name, .. } => {
            if st.env_lookup(name).is_some() || defined.contains(name) {
                return ConstFold::NotConst;
            }
            if let Some(v) = consts.get(name.as_str()) {
                return divisor_int_fold(v, consts, st, defined, aliases, depth + 1);
            }
            ConstFold::NotConst
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = divisor_int_fold(lhs, consts, st, defined, aliases, depth + 1);
            let r = divisor_int_fold(rhs, consts, st, defined, aliases, depth + 1);
            crate::const_overflow::fold_bin(*op, l, r)
        }
        Expr::Call { args, .. } if args.len() == 1 && !args[0].spread => {
            // `int(0.0)`, `uint8(-0.0)`, `i64(1.5 - 1.5)`, `const fz = 0.0;
            // int(fz)`: Go folds the float constant and converts it, so the
            // divisor IS the integer zero `const_int_fold_with` refuses to see.
            // The alias spelling (`type my_int = int; x / my_int(0)`) misses the
            // shared conversion table for the same reason (#2426).
            if let Some(arg) = int_conversion_arg(bare, aliases) {
                if matches!(
                    crate::const_overflow::const_float_fold_with(arg, consts, 0, Some(st)),
                    Some(f) if f == 0.0
                ) {
                    return ConstFold::Val(0);
                }
                return divisor_int_fold(arg, consts, st, defined, aliases, depth + 1);
            }
            // `real(complex(0, 5))` / `imag(complex(5, 0))` — the report names
            // both folders, and the int side starts here (#2426).
            if let Some(part) = complex_component(bare) {
                return divisor_int_fold(part, consts, st, defined, aliases, depth + 1);
            }
            const_int_fold_with(e, consts, depth, Some(st))
        }
        _ => const_int_fold_with(e, consts, depth, Some(st)),
    }
}

/// The argument of an integer conversion, when the callee names a predeclared
/// integer type or a file-level alias chain that ends at one (`int(0.0)`,
/// `my_int(7)`) (#2426).
fn int_conversion_arg<'e>(
    e: &'e Expr,
    aliases: &std::collections::HashMap<String, TypeExpr>,
) -> Option<&'e Expr> {
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if args.len() == 1 && !args[0].spread {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if is_int_conversion_name(name, aliases) {
                    return Some(unwrap_parens(&args[0].value));
                }
            }
        }
    }
    None
}

/// Is `name` an integer type — a predeclared spelling Go treats as an integer
/// conversion, or a file-level alias that resolves to one? (#2426)
fn is_int_conversion_name(name: &str, aliases: &std::collections::HashMap<String, TypeExpr>) -> bool {
    if crate::emit_guard::is_int_primitive_type_name(name) {
        return true;
    }
    match aliases.get(name) {
        Some(ty) => match resolve_alias_type(ty, aliases) {
            TypeExpr::Named { name: target, .. } => {
                crate::emit_guard::is_int_primitive_type_name(&target)
            }
            _ => false,
        },
        None => false,
    }
}

/// Follow `type a = b` chains with a depth cap. Anything that is not a named
/// type — or a name with no alias entry — comes back unchanged.
fn resolve_alias_type(ty: &TypeExpr, aliases: &std::collections::HashMap<String, TypeExpr>) -> TypeExpr {
    let mut cur: TypeExpr = ty.clone();
    for _ in 0..8 {
        let next: Option<TypeExpr> = match &cur {
            TypeExpr::Named { name, .. } => aliases.get(name).cloned(),
            _ => None,
        };
        match next {
            Some(t) => cur = t,
            None => break,
        }
    }
    cur
}

fn divisor_int_fold_is_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
) -> bool {
    matches!(
        divisor_int_fold(e, consts, st, defined, aliases, 0),
        crate::const_overflow::ConstFold::Val(0)
    )
}

/// `real(complex(re, im))` -> `re`, `imag(complex(re, im))` -> `im`: Go evaluates
/// the selected component of a constant complex value at compile time, so a zero
/// component is a constant zero divisor (#2426).
fn complex_component(e: &Expr) -> Option<&Expr> {
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if (name == "real" || name == "imag") && args.len() == 1 && !args[0].spread {
                if let Expr::Call {
                    callee: inner,
                    args: inner_args,
                    ..
                } = unwrap_parens(&args[0].value)
                {
                    if let Expr::Ident { name: inner_name, .. } = unwrap_parens(inner) {
                        if inner_name == "complex" && inner_args.len() == 2 {
                            return Some(if name == "real" {
                                unwrap_parens(&inner_args[0].value)
                            } else {
                                unwrap_parens(&inner_args[1].value)
                            });
                        }
                    }
                }
            }
        }
    }
    None
}

/// `len`/`cap` of a fixed-size array is a Go constant, so a zero-length array is
/// a constant zero divisor. Both spellings the report names are covered: the
/// typed array literal (`len([0]int{})`) and a binding declared with an array
/// type (`var a [0]int; x / len(a)`). Neither operand contains a call or a
/// channel receive, which is what makes the Go rule apply (#2426).
fn const_len_cap_value(
    e: &Expr,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
    depth: u32,
) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if (name == "len" || name == "cap") && args.len() == 1 && !args[0].spread {
                let operand = unwrap_parens(&args[0].value);
                // `[N]T{}` — the parser keeps the array type on the literal.
                if let Some((Some(len_expr), _, _)) = operand.as_array_lit() {
                    if let Some(n) = const_eval_int(len_expr) {
                        return Some(n);
                    }
                }
                // `len(a)` where `a` was declared `[N]T`.
                if let Expr::Ident { name: n, .. } = operand {
                    if !defined.contains(n) {
                        if let Some(ty) = st.env_lookup(n).and_then(|b| b.ty.as_ref()) {
                            if let TypeExpr::Array {
                                len: Some(len_expr),
                                ..
                            } = resolve_alias_type(ty, aliases)
                            {
                                return const_eval_int(&len_expr);
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Float-conversion zero leaves: `float64(0)`, `f64(1 - 1)` — the float
/// mirror of the int-conversion arm in `const_int_fold_with` (#725).
fn divisor_float_conv_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
    depth: u32,
) -> bool {
    if depth > 16 {
        return false;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if args.len() == 1 && !args[0].spread {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if matches!(name.as_str(), "f32" | "f64" | "float32" | "float64") {
                    let a = &args[0].value;
                    if divisor_int_fold_is_zero(a, consts, st, defined, aliases) {
                        return true;
                    }
                    if matches!(
                        crate::const_overflow::const_float_fold_with(a, consts, 0, Some(st)),
                        Some(f) if f == 0.0
                    ) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn divisor_float_fold_is_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
) -> bool {
    // `real(complex(0, 5))` / `imag(complex(5, 0))`: the selected component of a
    // constant complex value is a constant, so a zero component is a constant
    // zero divisor on this side too (#2426).
    if let Some(part) = complex_component(e) {
        if divisor_float_fold_is_zero(part, consts, st, defined, aliases) {
            return true;
        }
    }
    if divisor_float_conv_zero(e, consts, st, defined, aliases, 0) {
        return true;
    }
    matches!(
        crate::const_overflow::const_float_fold_with(e, consts, 0, Some(st)),
        Some(f) if f == 0.0
    )
}

/// Const-ness of the float-branch dividend, map-aware: the syntactic rule
/// (`is_const_expr`) plus anything the int/float folds can prove.
fn is_const_map_expr(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    aliases: &std::collections::HashMap<String, TypeExpr>,
) -> bool {
    if is_const_expr(e) {
        return true;
    }
    !matches!(
        divisor_int_fold(e, consts, st, defined, aliases, 0),
        crate::const_overflow::ConstFold::NotConst
    ) || crate::const_overflow::const_float_fold_with(e, consts, 0, Some(st)).is_some()
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
