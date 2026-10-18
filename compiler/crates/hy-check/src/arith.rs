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
                self.check_expr_const_zero_divisor(&c.value);
            }
        }
    }

    pub(crate) fn check_stmt_const_zero_divisor(&mut self, stmt: &Stmt) {
        self.check_stmt_const_zero_divisor_inner(stmt, /*deep=*/ false);
    }

    fn check_stmt_const_zero_divisor_inner(&mut self, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
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
                    && is_int_literal_zero(&values[0])
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
            Stmt::Managed { body, .. } | Stmt::Split { body, .. } => {
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
                if matches!(op, BinOp::Div | BinOp::Mod) && is_int_literal_zero(rhs) {
                    self.emit_const_zero_divisor(*span, bin_op_txt(*op));
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
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_const_zero_divisor(&f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for i in items {
                    self.check_expr_const_zero_divisor(i);
                }
            }
            Expr::MapLit { entries, .. } => {
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
            Expr::FnLit { body, .. } => {
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
