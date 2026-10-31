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
    Stmt, TypeExpr, UnOp,
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
                {
                    let rhs = &values[0];
                    let rhs_is_int_zero = is_int_literal_zero(rhs)
                        || self.divisor_int_fold_is_zero(rhs, st);
                    let target_is_int = targets.first().map_or(false, |t| self.place_is_integer(st, t));
                    let rhs_is_untyped_float = self.is_untyped_float_zero(rhs, st);

                    if rhs_is_int_zero || (rhs_is_untyped_float && target_is_int) {
                        self.emit_const_zero_divisor(*span, assign_op_txt(*op));
                    }
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
                    let rhs_is_int_zero = self.divisor_int_fold_is_zero(rhs, st);
                    let rhs_is_float_literal_zero = is_float_literal_zero(rhs);
                    let rhs_is_float_zero = rhs_is_float_literal_zero
                        || self.divisor_float_fold_is_zero(rhs, st);

                    let lhs_is_int = self.expr_is_integer(st, lhs);
                    let lhs_is_const = is_const_expr(lhs) || self.is_const_map_expr(lhs, st);
                    let rhs_is_untyped_float = self.is_untyped_float_zero(rhs, st);

                    if rhs_is_int_zero
                        || (rhs_is_float_zero && lhs_is_const)
                        || (rhs_is_untyped_float && lhs_is_int)
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

fn is_builtin_int_type(name: &str) -> bool {
    matches!(
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
    )
}

fn contains_call_or_receive(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Call { .. } | Expr::Receive { .. } => true,
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. } => contains_call_or_receive(operand),
        Expr::Binary { lhs, rhs, .. } => {
            contains_call_or_receive(lhs) || contains_call_or_receive(rhs)
        }
        Expr::Index { base, index, .. } => {
            contains_call_or_receive(base) || contains_call_or_receive(index)
        }
        Expr::Slice { base, lo, hi, .. } => {
            contains_call_or_receive(base)
                || lo.as_deref().map_or(false, contains_call_or_receive)
                || hi.as_deref().map_or(false, contains_call_or_receive)
        }
        Expr::SliceLit { items, .. } => items.iter().any(contains_call_or_receive),
        _ => false,
    }
}

fn is_untyped_float_expr(e: &Expr, consts: &std::collections::HashMap<String, &Expr>) -> bool {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Float(_),
            ..
        } => true,
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => is_untyped_float_expr(operand, consts),
        Expr::Binary { lhs, rhs, .. } => {
            is_untyped_float_expr(lhs, consts) && is_untyped_float_expr(rhs, consts)
        }
        Expr::Ident { name, .. } => {
            if let Some(c) = consts.get(name.as_str()) {
                is_untyped_float_expr(c, consts)
            } else {
                false
            }
        }
        Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if name == "real" || name == "imag" {
                    return is_untyped_complex_expr(&args[0].value, consts);
                }
            }
            false
        }
        _ => false,
    }
}

fn is_untyped_complex_expr(e: &Expr, consts: &std::collections::HashMap<String, &Expr>) -> bool {
    match unwrap_parens(e) {
        Expr::Call { callee, args, .. }
            if args.len() == 2 && !args[0].spread && !args[1].spread =>
        {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if name == "complex" {
                    return true;
                }
            }
            false
        }
        Expr::Ident { name, .. } => {
            if let Some(c) = consts.get(name.as_str()) {
                is_untyped_complex_expr(c, consts)
            } else {
                false
            }
        }
        _ => false,
    }
}

fn scan_block_for_int_binding(block: &hy_syntax::Block, name: &str) -> Option<bool> {
    for s in &block.stmts {
        match s {
            Stmt::Local(l) => {
                for (n, v) in l.names.iter().zip(l.values.iter()) {
                    if n == name {
                        if let Some(t) = &l.ty {
                            if let TypeExpr::Named { name: tname, .. } = t {
                                return Some(is_builtin_int_type(tname));
                            }
                        }
                        if let Expr::Lit { kind, .. } = unwrap_parens(v) {
                            return Some(matches!(kind, LitKind::Int(_) | LitKind::Char(_)));
                        }
                    }
                }
            }
            Stmt::Assign { targets, values, .. } => {
                for (t, v) in targets.iter().zip(values.iter()) {
                    if let PlaceKind::Ident(n) = &t.kind {
                        if n == name {
                            if let Expr::Lit { kind, .. } = unwrap_parens(v) {
                                return Some(matches!(kind, LitKind::Int(_) | LitKind::Char(_)));
                            }
                        }
                    }
                }
            }
            Stmt::Block { block: b, .. } => {
                if let Some(res) = scan_block_for_int_binding(b, name) {
                    return Some(res);
                }
            }
            Stmt::If {
                then,
                otherwise,
                ..
            } => {
                if let Some(res) = scan_block_for_int_binding(then, name) {
                    return Some(res);
                }
                if let Some(el) = otherwise {
                    match el.as_ref() {
                        Stmt::Block { block: b, .. } => {
                            if let Some(res) = scan_block_for_int_binding(b, name) {
                                return Some(res);
                            }
                        }
                        _ => {}
                    }
                }
            }
            Stmt::For { body, .. } => {
                if let Some(res) = scan_block_for_int_binding(body, name) {
                    return Some(res);
                }
            }
            Stmt::Managed { body, .. } => {
                if let Some(res) = scan_block_for_int_binding(body, name) {
                    return Some(res);
                }
            }
            _ => {}
        }
    }
    None
}

impl<'a> Checker<'a> {
    pub(crate) fn resolves_to_int_type(&self, name: &str, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        if is_builtin_int_type(name) {
            return true;
        }
        if let Some(TypeExpr::Named { name: target, .. }) = self.type_aliases.get(name) {
            return self.resolves_to_int_type(target, depth + 1);
        }
        false
    }

    pub(crate) fn type_is_integer(&self, ty: &TypeExpr, depth: usize) -> bool {
        if depth > 8 {
            return false;
        }
        match ty {
            TypeExpr::Named { name, .. } => self.resolves_to_int_type(name, depth + 1),
            _ => false,
        }
    }

    pub(crate) fn expr_is_integer(&self, st: &crate::model::FnState, e: &Expr) -> bool {
        match unwrap_parens(e) {
            Expr::Lit {
                kind: LitKind::Int(_) | LitKind::Char(_),
                ..
            } => true,
            Expr::Ident { name, .. } => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        return self.type_is_integer(ty, 0);
                    }
                    if self.local_is_integer(name) {
                        return true;
                    }
                }
                if let Some(c) = self.const_values.get(name.as_str()) {
                    return self.expr_is_integer(st, c);
                }
                false
            }
            Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if self.resolves_to_int_type(name, 0) || name == "len" || name == "cap" {
                        return true;
                    }
                }
                false
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.expr_is_integer(st, lhs) || self.expr_is_integer(st, rhs)
            }
            other => {
                if let Some(ty) = self.ty_of_expr(st, other) {
                    self.type_is_integer(&ty, 0)
                } else {
                    false
                }
            }
        }
    }

    pub(crate) fn place_is_integer(&self, st: &crate::model::FnState, p: &Place) -> bool {
        match &p.kind {
            PlaceKind::Ident(name) => {
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        return self.type_is_integer(ty, 0);
                    }
                    if self.local_is_integer(name) {
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn local_is_integer(&self, name: &str) -> bool {
        for decl in &self.file.decls {
            if let hy_syntax::Decl::Fn(f) = decl {
                if let Some(body) = &f.body {
                    if let Some(is_int) = scan_block_for_int_binding(body, name) {
                        return is_int;
                    }
                }
            }
        }
        false
    }

    fn const_str_len(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
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
            Expr::Binary {
                op: BinOp::Add,
                lhs,
                rhs,
                ..
            } => {
                let l = self.const_str_len(lhs, st, depth + 1)?;
                let r = self.const_str_len(rhs, st, depth + 1)?;
                l.checked_add(r)
            }
            Expr::Ident { name, .. } => {
                if st.env_lookup(name).is_some() || self.zd_defined.contains(name) {
                    return None;
                }
                if let Some(v) = self.const_values.get(name.as_str()) {
                    return self.const_str_len(v, st, depth + 1);
                }
                None
            }
            _ => None,
        }
    }

    fn const_array_len(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
        depth: u32,
    ) -> Option<i128> {
        if depth > 16 || contains_call_or_receive(e) {
            return None;
        }
        let bare = unwrap_parens(e);
        // 1. Literal [0]int{}
        if let Some((Some(len_expr), _, _)) = bare.as_array_lit() {
            return match self.divisor_int_fold(len_expr, st, depth + 1) {
                crate::const_overflow::ConstFold::Val(n) => Some(n),
                _ => None,
            };
        }
        // 2. Variable or binding lookup
        if let Expr::Ident { name, .. } = bare {
            if let Some(b) = self.lookup_binding(st, name) {
                if let Some(ty) = &b.ty {
                    return self.type_array_len(ty, st, depth + 1);
                }
            }
        }
        // 3. Fallback through ty_of_expr
        if let Some(ty) = self.ty_of_expr(st, bare) {
            return self.type_array_len(&ty, st, depth + 1);
        }
        None
    }

    fn type_array_len(
        &self,
        ty: &TypeExpr,
        st: &crate::model::FnState,
        depth: u32,
    ) -> Option<i128> {
        if depth > 16 {
            return None;
        }
        match ty {
            TypeExpr::Array {
                len: Some(len_expr),
                ..
            } => match self.divisor_int_fold(len_expr, st, depth + 1) {
                crate::const_overflow::ConstFold::Val(n) => Some(n),
                _ => None,
            },
            TypeExpr::Pointer { to, .. } => self.type_array_len(to, st, depth + 1),
            TypeExpr::Named { name, .. } => {
                if let Some(aliased) = self.type_aliases.get(name) {
                    self.type_array_len(aliased, st, depth + 1)
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn extract_complex_part(
        &self,
        callee_name: &str,
        arg: &Expr,
        st: &crate::model::FnState,
        depth: u32,
    ) -> Option<Expr> {
        if depth > 16 {
            return None;
        }
        let bare = unwrap_parens(arg);
        match bare {
            Expr::Call { callee, args, .. }
                if args.len() == 2 && !args[0].spread && !args[1].spread =>
            {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "complex" {
                        if callee_name == "real" {
                            return Some(args[0].value.clone());
                        } else if callee_name == "imag" {
                            return Some(args[1].value.clone());
                        }
                    }
                }
                None
            }
            Expr::Ident { name, .. } => {
                if st.env_lookup(name).is_some() || self.zd_defined.contains(name) {
                    return None;
                }
                if let Some(v) = self.const_values.get(name.as_str()) {
                    return self.extract_complex_part(callee_name, v, st, depth + 1);
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn divisor_int_fold(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
        depth: u32,
    ) -> crate::const_overflow::ConstFold {
        use crate::const_overflow::{const_int_fold_with, ConstFold};
        if depth > 16 {
            return ConstFold::NotConst;
        }
        let bare = unwrap_parens(e);
        if let Expr::Call { callee, args, .. } = bare {
            if args.len() == 1 && !args[0].spread {
                if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                    if name == "len" {
                        if let Some(n) = self.const_str_len(&args[0].value, st, depth + 1) {
                            return ConstFold::Val(n);
                        }
                        if let Some(n) = self.const_array_len(&args[0].value, st, depth + 1) {
                            return ConstFold::Val(n);
                        }
                    } else if name == "cap" {
                        if let Some(n) = self.const_array_len(&args[0].value, st, depth + 1) {
                            return ConstFold::Val(n);
                        }
                    } else if name == "real" || name == "imag" {
                        if let Some(part) = self.extract_complex_part(name, &args[0].value, st, depth + 1) {
                            let ifold = self.divisor_int_fold(&part, st, depth + 1);
                            if !matches!(ifold, ConstFold::NotConst) {
                                return ifold;
                            }
                            if let Some(f) = crate::const_overflow::const_float_fold_with(
                                &part,
                                &self.const_values,
                                depth + 1,
                                Some(st),
                            ) {
                                if f.is_finite() && f == 0.0 {
                                    return ConstFold::Val(0);
                                }
                                if f.is_finite() && f.trunc() == f {
                                    return ConstFold::Val(f as i128);
                                }
                            }
                        }
                    } else if self.resolves_to_int_type(name, 0) {
                        let arg = &args[0].value;
                        let ifold = self.divisor_int_fold(arg, st, depth + 1);
                        if let ConstFold::Val(v) = ifold {
                            if let Some((lo, hi)) = crate::const_overflow::fixed_width_int_range(name) {
                                return ConstFold::Val(crate::const_overflow::wrap_into(v, lo, hi));
                            }
                            return ConstFold::Val(v);
                        }
                        if let Some(f) = crate::const_overflow::const_float_fold_with(
                            arg,
                            &self.const_values,
                            depth + 1,
                            Some(st),
                        ) {
                            if f.is_finite() && f == 0.0 {
                                return ConstFold::Val(0);
                            }
                            if f.is_finite() && f.trunc() == f {
                                let v = f as i128;
                                if let Some((lo, hi)) = crate::const_overflow::fixed_width_int_range(name) {
                                    return ConstFold::Val(crate::const_overflow::wrap_into(v, lo, hi));
                                }
                                return ConstFold::Val(v);
                            }
                        }
                    }
                }
            }
        }
        match bare {
            Expr::Ident { name, .. } => {
                if st.env_lookup(name).is_some() || self.zd_defined.contains(name) {
                    return ConstFold::NotConst;
                }
                if let Some(v) = self.const_values.get(name.as_str()) {
                    return self.divisor_int_fold(v, st, depth + 1);
                }
                ConstFold::NotConst
            }
            Expr::Binary { op, lhs, rhs, .. } => {
                let l = self.divisor_int_fold(lhs, st, depth + 1);
                let r = self.divisor_int_fold(rhs, st, depth + 1);
                crate::const_overflow::fold_bin(*op, l, r)
            }
            _ => const_int_fold_with(e, &self.const_values, depth, Some(st)),
        }
    }

    pub(crate) fn divisor_int_fold_is_zero(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
    ) -> bool {
        matches!(
            self.divisor_int_fold(e, st, 0),
            crate::const_overflow::ConstFold::Val(0)
        )
    }

    pub(crate) fn divisor_float_conv_zero(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
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
                        if self.divisor_int_fold_is_zero(a, st) {
                            return true;
                        }
                        if matches!(
                            crate::const_overflow::const_float_fold_with(a, &self.const_values, 0, Some(st)),
                            Some(f) if f == 0.0
                        ) {
                            return true;
                        }
                    } else if name == "real" || name == "imag" {
                        if let Some(part) = self.extract_complex_part(name, &args[0].value, st, depth + 1) {
                            if matches!(
                                crate::const_overflow::const_float_fold_with(&part, &self.const_values, 0, Some(st)),
                                Some(f) if f == 0.0
                            ) {
                                return true;
                            }
                            if self.divisor_int_fold_is_zero(&part, st) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }

    pub(crate) fn divisor_float_fold_is_zero(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
    ) -> bool {
        if self.divisor_float_conv_zero(e, st, 0) {
            return true;
        }
        matches!(
            crate::const_overflow::const_float_fold_with(e, &self.const_values, 0, Some(st)),
            Some(f) if f == 0.0
        )
    }

    pub(crate) fn is_const_map_expr(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
    ) -> bool {
        if is_const_expr(e) {
            return true;
        }
        !matches!(
            self.divisor_int_fold(e, st, 0),
            crate::const_overflow::ConstFold::NotConst
        ) || crate::const_overflow::const_float_fold_with(e, &self.const_values, 0, Some(st)).is_some()
    }

    pub(crate) fn is_untyped_float_zero(
        &self,
        e: &Expr,
        st: &crate::model::FnState,
    ) -> bool {
        is_untyped_float_expr(e, &self.const_values)
            && (is_float_literal_zero(e) || self.divisor_float_fold_is_zero(e, st))
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
