//! const_overflow.rs — constant initializers that would make the emitted
//! Go unbuildable (issue #148, extended by issue #201-B).
//!
//! `var n int8 = 300` (and the file-level `const` twin) pass `hyb check`
//! with 0 diagnostics today, then fail `go vet` / `go build` with
//! `cannot use 300 as int8 value (overflows)`. The accepted-should-
//! emit-buildable-Go contract requires a checker diagnostic (HYB-E6002)
//! for that narrow constant case. Issue #201-B extends the fold to
//! constant *expressions* (`100 + 100`, `1 << 10` — Go folds them before
//! the range check, so they overflow the same way) and to float constants
//! assigned to an integer type (`2.5` → Go `(truncated)`). Non-constant
//! operands stay out of scope — Go truncates at run time via an explicit
//! conversion (SPEC-LANGUAGE X4), and the conversion form `int8(x)` is
//! unaffected.

use crate::model::{unwrap_parens, Checker};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{BinOp, Expr, LitKind, TypeExpr, UnOp};

impl<'a> Checker<'a> {
    /// When `ty` is a fixed-width integer and `value` is an integer
    /// constant (literal or folded constant expression) outside that
    /// type's range, or a float constant, emit HYB-E6002.
    pub(crate) fn check_const_overflow_init(&mut self, name: &str, ty: &TypeExpr, value: &Expr) {
        let TypeExpr::Named {
            name: ty_name,
            span: ty_span,
        } = ty
        else {
            return;
        };
        let Some((lo, hi)) = fixed_width_int_range(ty_name) else {
            return;
        };
        match const_int_fold(value) {
            ConstFold::NotConst => {}
            ConstFold::TooBig => {
                let span = value.span();
                let msg = format!(
                    "constant expression `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    name
                );
                let notes = vec![
                    format!(
                        "why: Go folds the constant expression before the range check and rejects a value outside `{}` (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}] or widen the declared type",
                        lo, hi
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
            ConstFold::Val(v) => {
                if v >= lo && v <= hi {
                    return;
                }
                let span = value.span();
                let msg = format!(
                    "constant `{}` overflows declared type `{}` of `{}` (HYB-E6002)",
                    value.render(),
                    ty_name,
                    name
                );
                let notes = vec![
                    format!(
                        "why: Go rejects a constant initializer outside `{}`'s range (`cannot use … as {} value (overflows)`), so an accepted Hybrid program would emit unbuildable Go (issue #148)",
                        ty_name, ty_name
                    ),
                    format!(
                        "fix: use a constant in [{}, {}], widen the declared type, or convert from a variable if truncation is intentional (`x := {}; y := {}(x)`)",
                        lo, hi, value.render(), ty_name
                    ),
                    format!(
                        "note: the declared type was written here ({})",
                        span_hint(*ty_span)
                    ),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
                );
                return;
            }
        }
        // Issue #201-B: a float constant is never assignable to an integer
        // type in Go — `var x int = 2.5` fails with `(truncated)` (and a
        // magnitude that cannot fit fails with `(overflows)`).
        if let Some(f) = const_float_value(value) {
            let span = value.span();
            let msg = format!(
                "float constant `{}` assigned to integer type `{}` of `{}` (HYB-E6002)",
                value.render(),
                ty_name,
                name
            );
            let notes = vec![
                format!(
                    "why: Go rejects a float constant as an `{}` value (`cannot use … as {} value (truncated)`), so an accepted Hybrid program would emit unbuildable Go (issue #201)",
                    ty_name, ty_name
                ),
                format!(
                    "fix: drop the fractional part (`{}` → `{}`), use a float declared type, or convert from a variable if rounding is intentional",
                    value.render(),
                    integer_of_float(f)
                ),
                format!(
                    "note: the declared type was written here ({})",
                    span_hint(*ty_span)
                ),
            ];
            self.diags.push(
                Diagnostic::error("HYB-E6002", self.path.clone(), span, msg).with_notes(notes),
            );
        }
    }
}

fn span_hint(span: Span) -> String {
    format!("{}:{}", span.start.line, span.start.col)
}

/// Inclusive representable range for a Hybrid fixed-width integer type
/// name (SPEC-LANGUAGE §11 / Go's typed-constant assignment rule).
/// Platform-word `int`/`uint`/`isize`/`usize` use the 64-bit LP64 ranges
/// the emitter targets (SPEC B9 / X7).
fn fixed_width_int_range(name: &str) -> Option<(i128, i128)> {
    Some(match name {
        "i8" | "int8" => (i8::MIN as i128, i8::MAX as i128),
        "u8" | "uint8" | "byte" => (0, u8::MAX as i128),
        "i16" | "int16" => (i16::MIN as i128, i16::MAX as i128),
        "u16" | "uint16" => (0, u16::MAX as i128),
        "i32" | "int32" | "rune" => (i32::MIN as i128, i32::MAX as i128),
        "u32" | "uint32" => (0, u32::MAX as i128),
        "i64" | "int64" | "int" | "isize" => (i64::MIN as i128, i64::MAX as i128),
        "u64" | "uint64" | "uint" | "usize" | "uintptr" => (0, u64::MAX as i128),
        _ => return None,
    })
}

/// Outcome of folding a constant integer expression (issue #201-B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstFold {
    /// Folded to an exact i128 value.
    Val(i128),
    /// Not a constant the checker can prove (idents, calls, …) — callers
    /// stay silent (fail-open; other diagnostics own those shapes).
    NotConst,
    /// Provably a constant whose magnitude exceeds every fixed-width
    /// Hybrid integer range (e.g. `1 << 200`, `i128` arithmetic overflow).
    TooBig,
}

/// Fold a pure integer constant expression: int literals (any base the
/// lexer keeps), unary `-`, parentheses, and the arithmetic/shift/bitwise
/// operators over constant operands (issue #201-B: Go folds `100 + 100`
/// and `1 << 10` before the range check, so they overflow identically).
/// Anything else (idents, calls) is `NotConst`.
pub(crate) fn const_int_fold(e: &Expr) -> ConstFold {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Int(s),
            ..
        } => match parse_int_lit(s) {
            Some(v) => ConstFold::Val(v),
            None => ConstFold::NotConst,
        },
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => match const_int_fold(operand) {
            ConstFold::Val(v) => v.checked_neg().map(ConstFold::Val).unwrap_or(ConstFold::TooBig),
            ConstFold::NotConst => ConstFold::NotConst,
            ConstFold::TooBig => ConstFold::TooBig,
        },
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_int_fold(lhs);
            let r = const_int_fold(rhs);
            fold_bin(*op, l, r)
        }
        _ => ConstFold::NotConst,
    }
}

fn fold_bin(op: BinOp, l: ConstFold, r: ConstFold) -> ConstFold {
    use BinOp::*;
    // Division/shift-by-huge and `TooBig` interactions are handled per-op.
    match op {
        Add | Sub | Mul => {
            if matches!(l, ConstFold::NotConst) || matches!(r, ConstFold::NotConst) {
                return ConstFold::NotConst;
            }
            if matches!(l, ConstFold::TooBig) || matches!(r, ConstFold::TooBig) {
                return ConstFold::TooBig;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            let v = match op {
                Add => a.checked_add(b),
                Sub => a.checked_sub(b),
                _ => a.checked_mul(b),
            };
            v.map(ConstFold::Val).unwrap_or(ConstFold::TooBig)
        }
        // Bitwise masking can shrink a `TooBig` value back into range —
        // give up (fail-open) rather than guess.
        BitAnd | BitOr | BitXor => {
            if matches!(l, ConstFold::TooBig) || matches!(r, ConstFold::TooBig) {
                return ConstFold::NotConst;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            let v = match op {
                BitAnd => a & b,
                BitOr => a | b,
                _ => a ^ b,
            };
            ConstFold::Val(v)
        }
        Shl => {
            let (ConstFold::Val(a), ConstFold::Val(n)) = (l, r) else {
                // A provably-huge left operand stays huge when shifted by
                // a non-negative constant; anything else fails open.
                if matches!(l, ConstFold::TooBig)
                    && matches!(r, ConstFold::Val(n) if n >= 0)
                {
                    return ConstFold::TooBig;
                }
                return ConstFold::NotConst;
            };
            if n < 0 {
                // Negative counts are HYB-E6005's job at the shift site.
                return ConstFold::NotConst;
            }
            if a == 0 {
                return ConstFold::Val(0);
            }
            if n >= 128 {
                return ConstFold::TooBig;
            }
            let n = n as u32;
            let base = 1u128 << n;
            let Some(mag) = (a.unsigned_abs() as u128).checked_mul(base) else {
                return ConstFold::TooBig;
            };
            if mag > i128::MAX as u128 {
                return ConstFold::TooBig;
            }
            let mag = mag as i128;
            ConstFold::Val(if a < 0 { -mag } else { mag })
        }
        Shr => {
            let (ConstFold::Val(a), ConstFold::Val(n)) = (l, r) else {
                return ConstFold::NotConst;
            };
            if n < 0 {
                return ConstFold::NotConst;
            }
            if n >= 128 {
                // Arithmetic shift: non-negative → 0, negative → -1.
                return ConstFold::Val(if a < 0 { -1 } else { 0 });
            }
            ConstFold::Val(a >> (n as u32))
        }
        Div | Mod => {
            // `TooBig` operands give up: division shrinks and could land
            // back in range — never guess (fail-open, no false reject).
            if !matches!(l, ConstFold::Val(_)) || !matches!(r, ConstFold::Val(_)) {
                return ConstFold::NotConst;
            }
            let (ConstFold::Val(a), ConstFold::Val(b)) = (l, r) else {
                return ConstFold::NotConst;
            };
            if b == 0 {
                return ConstFold::NotConst; // HYB-E6001 owns `/ 0`
            }
            match op {
                Div => a.checked_div(b).map(ConstFold::Val).unwrap_or(ConstFold::TooBig),
                _ => a.checked_rem(b).map(ConstFold::Val).unwrap_or(ConstFold::NotConst),
            }
        }
        _ => ConstFold::NotConst,
    }
}

/// Convenience: fold to a value when the expression IS a constant in
/// range (used by the negative-shift check in emit_guard.rs).
pub(crate) fn const_int_value(e: &Expr) -> Option<i128> {
    match const_int_fold(e) {
        ConstFold::Val(v) => Some(v),
        _ => None,
    }
}

/// Fold a float constant expression (literals, unary `-`, parens) — the
/// `var x int = 2.5` truncation site (issue #201-B).
pub(crate) fn const_float_value(e: &Expr) -> Option<f64> {
    match unwrap_parens(e) {
        Expr::Lit {
            kind: LitKind::Float(s),
            ..
        } => s.replace('_', "").parse::<f64>().ok(),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => const_float_value(operand).map(|v| -v),
        _ => None,
    }
}

/// Render a folded float as the nearest integer literal for the fix-it.
fn integer_of_float(f: f64) -> String {
    format!("{}", f.trunc() as i128)
}

fn parse_int_lit(s: &str) -> Option<i128> {
    let s = s.as_bytes();
    if s.is_empty() {
        return None;
    }
    if s.len() >= 2 && s[0] == b'0' {
        match s[1].to_ascii_lowercase() {
            b'x' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 16).ok(),
            b'b' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 2).ok(),
            b'o' => return i128::from_str_radix(std::str::from_utf8(&s[2..]).ok()?, 8).ok(),
            _ => {}
        }
    }
    i128::from_str_radix(std::str::from_utf8(s).ok()?, 10).ok()
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn ranges_match_go_fixed_width() {
        assert_eq!(fixed_width_int_range("int8"), Some((-128, 127)));
        assert_eq!(fixed_width_int_range("byte"), Some((0, 255)));
        assert_eq!(fixed_width_int_range("u16"), Some((0, 65535)));
        assert!(fixed_width_int_range("string").is_none());
        assert!(fixed_width_int_range("float64").is_none());
    }

    #[test]
    fn parse_bases() {
        assert_eq!(parse_int_lit("300"), Some(300));
        assert_eq!(parse_int_lit("0x12C"), Some(300));
        assert_eq!(parse_int_lit("0b101"), Some(5));
        assert_eq!(parse_int_lit("0o10"), Some(8));
    }
}
