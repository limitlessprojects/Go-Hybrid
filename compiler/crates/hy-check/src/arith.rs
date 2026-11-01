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
                        || divisor_int_fold_is_zero(
                            &values[0],
                            self,
                            st,
                            &self.zd_defined,
                        ))
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
                    if divisor_int_fold_is_zero(rhs, self, st, &self.zd_defined)
                        || is_float_literal_zero(rhs) && is_const_expr(lhs)
                        || divisor_float_fold_is_zero(rhs, self, st, &self.zd_defined)
                            && is_const_map_expr(lhs, self, st, &self.zd_defined)
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
                let mut defined = self.zd_defined.clone();
                for p in params {
                    defined.insert(p.name.clone());
                }
                for stmt in &body.stmts {
                    stmt_defined_names(stmt, &mut defined);
                }
                let prev_defined = std::mem::replace(&mut self.zd_defined, defined);
                for stmt in &body.stmts {
                    self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ true);
                }
                self.zd_defined = prev_defined;
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
        Expr::Binary { lhs, rhs, .. } => {
            expr_defined_names(lhs, out);
            expr_defined_names(rhs, out);
        }
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
                if let hy_syntax::MatchPattern::Path {
                    binding: Some(b), ..
                } = &arm.pattern
                {
                    out.insert(b.clone());
                }
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

fn resolve_unshadowed_conv_target<'a>(
    name: &'a str,
    ck: &'a Checker<'a>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> Option<&'a str> {
    let mut cur = name;
    for _ in 0..8 {
        if st.env_lookup(cur).is_some()
            || defined.contains(cur)
            || ck.fns.contains_key(cur)
            || ck.structs.contains_key(cur)
            || ck.struct_type_params.contains_key(cur)
            || ck.enums.contains_key(cur)
            || ck.consts.contains_key(cur)
        {
            return None;
        }
        if let Some(hy_syntax::TypeExpr::Named { name: next, .. }) = ck.type_aliases.get(cur) {
            cur = next.as_str();
            continue;
        }
        if ck.type_aliases.contains_key(cur) {
            return None;
        }
        return Some(cur);
    }
    None
}

fn int_conv_fits(target: &str, v: i128) -> bool {
    match target {
        "i8" | "int8" => (i8::MIN as i128..=i8::MAX as i128).contains(&v),
        "i16" | "int16" => (i16::MIN as i128..=i16::MAX as i128).contains(&v),
        "i32" | "int32" | "rune" | "char" => (i32::MIN as i128..=i32::MAX as i128).contains(&v),
        "i64" | "int64" | "int" | "isize" => (i64::MIN as i128..=i64::MAX as i128).contains(&v),
        "u8" | "uint8" | "byte" => (0..=u8::MAX as i128).contains(&v),
        "u16" | "uint16" => (0..=u16::MAX as i128).contains(&v),
        "u32" | "uint32" => (0..=u32::MAX as i128).contains(&v),
        "u64" | "uint64" | "uint" | "usize" | "uintptr" => (0..=u64::MAX as i128).contains(&v),
        _ => false,
    }
}

fn eval_const_str_bytes(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
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
        Expr::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
            ..
        } => Some(
            eval_const_str_bytes(lhs, ck, st, defined, depth + 1)?
                + eval_const_str_bytes(rhs, ck, st, defined, depth + 1)?,
        ),
        Expr::Ident { name, .. } if st.env_lookup(name).is_none() && !defined.contains(name) => {
            if let Some(b) = ck.consts.get(name.as_str()) {
                if !matches!(b.shape, crate::model::TypeShape::Str) {
                    return None;
                }
            }
            let v = ck.const_values.get(name.as_str())?;
            eval_const_str_bytes(v, ck, st, defined, depth + 1)
        }
        Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
            let Expr::Ident { name, .. } = unwrap_parens(callee) else {
                return None;
            };
            if resolve_unshadowed_conv_target(name, ck, st, defined) == Some("string") {
                eval_const_str_bytes(&args[0].value, ck, st, defined, depth + 1)
            } else {
                None
            }
        }
        _ => None,
    }
}

/// `len("abc")` — Go evaluates the length of a constant string at compile
/// time (spec: constant expression). Named const strings, concatenations, and
/// `string(...)` conversions resolve through the same map.
fn const_len_str_value(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if name == "len"
                && args.len() == 1
                && !args[0].spread
                && resolve_unshadowed_conv_target("len", ck, st, defined) == Some("len")
            {
                return eval_const_str_bytes(&args[0].value, ck, st, defined, depth + 1);
            }
        }
    }
    None
}

/// The #758 divisor fold: literals, named consts (shadow-checked against
/// the live env AND the deep-walk tree's defines), unary/binary arithmetic,
/// integer conversions over folded leaves, and `len()` of constant strings.
fn divisor_int_fold(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> crate::const_overflow::ConstFold {
    use crate::const_overflow::{const_int_fold_with, ConstFold};
    if depth > 16 {
        return ConstFold::NotConst;
    }
    let bare = unwrap_parens(e);
    if let Some(n) = const_len_str_value(bare, ck, st, defined, depth) {
        return ConstFold::Val(n);
    }
    match bare {
        Expr::Ident { name, .. } => {
            if st.env_lookup(name).is_some() || defined.contains(name) {
                return ConstFold::NotConst;
            }
            if let Some(v) = ck.const_values.get(name.as_str()) {
                return divisor_int_fold(v, ck, st, defined, depth + 1);
            }
            ConstFold::NotConst
        }
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => match divisor_int_fold(operand, ck, st, defined, depth + 1) {
            ConstFold::Val(v) => v.checked_neg().map_or(ConstFold::TooBig, ConstFold::Val),
            other => other,
        },
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = divisor_int_fold(lhs, ck, st, defined, depth + 1);
            let r = divisor_int_fold(rhs, ck, st, defined, depth + 1);
            crate::const_overflow::fold_bin(*op, l, r)
        }
        Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if let Some(target) = resolve_unshadowed_conv_target(name, ck, st, defined) {
                    if let ConstFold::Val(v) =
                        divisor_int_fold(&args[0].value, ck, st, defined, depth + 1)
                    {
                        return if int_conv_fits(target, v) {
                            ConstFold::Val(v)
                        } else {
                            ConstFold::NotConst
                        };
                    }
                }
            }
            ConstFold::NotConst
        }
        _ => const_int_fold_with(e, &ck.const_values, depth, Some(st)),
    }
}

fn divisor_int_fold_is_zero(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    matches!(
        divisor_int_fold(e, ck, st, defined, 0),
        crate::const_overflow::ConstFold::Val(0)
    )
}

fn divisor_float_fold(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> Option<f64> {
    if depth > 16 {
        return None;
    }
    let bare = unwrap_parens(e);
    if let Some(n) = const_len_str_value(bare, ck, st, defined, depth) {
        return Some(n as f64);
    }
    match bare {
        Expr::Lit {
            kind: LitKind::Float(raw),
            ..
        } => {
            if float_lit_is_zero(raw) {
                Some(0.0)
            } else {
                raw.chars().filter(|c| *c != '_').collect::<String>().parse::<f64>().ok()
            }
        }
        Expr::Lit {
            kind: LitKind::Int(raw),
            ..
        } => int_lit_value(raw).map(|v| v as f64),
        Expr::Lit {
            kind: LitKind::Char(c),
            ..
        } => Some(*c as u32 as f64),
        Expr::Ident { name, .. } => {
            if st.env_lookup(name).is_some() || defined.contains(name) {
                return None;
            }
            ck.const_values
                .get(name.as_str())
                .and_then(|v| divisor_float_fold(v, ck, st, defined, depth + 1))
        }
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => divisor_float_fold(operand, ck, st, defined, depth + 1).map(|v| -v),
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = divisor_float_fold(lhs, ck, st, defined, depth + 1)?;
            let r = divisor_float_fold(rhs, ck, st, defined, depth + 1)?;
            match op {
                BinOp::Add => Some(l + r),
                BinOp::Sub => Some(l - r),
                BinOp::Mul => Some(l * r),
                BinOp::Div if r != 0.0 => Some(l / r),
                _ => None,
            }
        }
        Expr::Call { callee, args, .. } if args.len() == 1 && !args[0].spread => {
            let Expr::Ident { name, .. } = unwrap_parens(callee) else {
                return None;
            };
            let target = resolve_unshadowed_conv_target(name, ck, st, defined)?;
            if matches!(target, "f32" | "f64" | "float32" | "float64") {
                if let crate::const_overflow::ConstFold::Val(n) =
                    divisor_int_fold(&args[0].value, ck, st, defined, depth + 1)
                {
                    return Some(n as f64);
                }
                return divisor_float_fold(&args[0].value, ck, st, defined, depth + 1);
            }
            if let crate::const_overflow::ConstFold::Val(n) =
                divisor_int_fold(bare, ck, st, defined, depth)
            {
                return Some(n as f64);
            }
            None
        }
        _ => crate::const_overflow::const_float_fold_with(e, &ck.const_values, depth, Some(st)),
    }
}

fn divisor_float_fold_is_zero(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    matches!(divisor_float_fold(e, ck, st, defined, 0), Some(f) if f == 0.0)
}

/// Const-ness of the float-branch dividend, map-aware: the syntactic rule
/// (`is_const_expr`) plus anything the int/float folds can prove.
fn is_const_map_expr(
    e: &Expr,
    ck: &Checker<'_>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    is_const_expr(e)
        || !matches!(
            divisor_int_fold(e, ck, st, defined, 0),
            crate::const_overflow::ConstFold::NotConst
        )
        || divisor_float_fold(e, ck, st, defined, 0).is_some()
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
