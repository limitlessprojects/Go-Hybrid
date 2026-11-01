//! SPEC-LANGUAGE line 1991 states an executable test-claim (§ "Examples are
//! tests" — a block with `// expect:` is run by the Phase-1 CI extractor and
//! its stdout diffed):
//!
//! ```hy
//! b := u8(300)               // expect: 44 (0x12C truncated to 0x2C)
//! ```
//!
//! Translating the name (`u8` -> `uint8`) is necessary but NOT sufficient: Go
//! rejects `uint8(300)` outright, because Go's rule is that a CONSTANT operand
//! must already be representable in the target type — the truncation Go does
//! for a *variable* operand does not apply to a constant.
//!
//! ```text
//! ./a.out.go:10:13: constant 300 overflows uint8
//! ```
//!
//! So the constant case has to be folded at emit time: the emitter computes
//! the two's-complement truncation itself and prints the already-truncated
//! literal, which is exactly what the SPEC's `// expect: 44` asserts.
//!
//! Only literals are folded, and only when the value does NOT already fit —
//! a value that fits is emitted untouched, so no currently-working program
//! changes shape.

use hy_syntax::LitKind;

/// `(bits, signed)` for a SPEC fixed-width primitive conversion target.
/// Go spellings are the same widths (the checker's fixed_width_int_range
/// treats `int8` == `i8`, `byte` == `u8`, `rune` == `i32`), so they fold
/// identically — `int8(300)` must truncate like `i8(300)` instead of
/// passing through and dying as `constant 300 overflows int8` (issue #228).
pub fn width_of(spec_name: &str) -> Option<(u32, bool)> {
    Some(match spec_name {
        "u8" | "byte" | "uint8" => (8, false),
        "u16" | "uint16" => (16, false),
        "u32" | "uint32" => (32, false),
        "u64" | "uint64" => (64, false),
        "i8" | "int8" => (8, true),
        "i16" | "int16" => (16, true),
        "i32" | "int32" | "rune" => (32, true),
        "i64" | "int64" => (64, true),
        _ => return None,
    })
}

/// Integer literal text -> value. Accepts the SPEC-LANGUAGE §LX6 forms:
/// decimal, `0x`/`0o`/`0b` prefixed, legacy leading-zero octal (`0755`), and
/// `_` digit separators.
pub fn parse_int(text: &str) -> Option<i128> {
    let t = text.replace('_', "");
    let (neg, t) = match t.strip_prefix('-') {
        Some(rest) => (true, rest.to_string()),
        None => (false, t),
    };
    let v = if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        i128::from_str_radix(h, 16).ok()?
    } else if let Some(o) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        i128::from_str_radix(o, 8).ok()?
    } else if let Some(b) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        i128::from_str_radix(b, 2).ok()?
    } else if t.len() > 1 && t.starts_with('0') && t.bytes().all(|c| (b'0'..=b'7').contains(&c)) {
        i128::from_str_radix(&t, 8).ok()?
    } else {
        t.parse::<i128>().ok()?
    };
    if neg { v.checked_neg() } else { Some(v) }
}

/// 512-bit signed two's-complement integer (matching Go's 512-bit constant
/// precision ceiling) so intermediate constant shifts past 127 bits (`(1 << 67)
/// << 60`, `(1 << 70) << 60`) neither sign-corrupt at bit 127 nor silently drop
/// high bits before a subsequent right shift or masking operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WideInt([u64; 8]);

impl WideInt {
    fn from_i128(v: i128) -> Self {
        let lo = v as u64;
        let hi = (v >> 64) as u64;
        let fill = if v < 0 { u64::MAX } else { 0 };
        Self([lo, hi, fill, fill, fill, fill, fill, fill])
    }

    fn to_i128(self) -> Option<i128> {
        let v = (((self.0[1] as u128) << 64) | (self.0[0] as u128)) as i128;
        let fill = if v < 0 { u64::MAX } else { 0 };
        if self.0[2..].iter().all(|&w| w == fill) {
            Some(v)
        } else {
            None
        }
    }

    fn is_neg(self) -> bool {
        (self.0[7] as i64) < 0
    }

    fn checked_add(self, rhs: Self) -> Option<Self> {
        let mut out = [0u64; 8];
        let mut carry = false;
        for i in 0..8 {
            let (s1, c1) = self.0[i].overflowing_add(rhs.0[i]);
            let (s2, c2) = s1.overflowing_add(u64::from(carry));
            out[i] = s2;
            carry = c1 || c2;
        }
        let res = Self(out);
        if (self.is_neg() == rhs.is_neg()) && (res.is_neg() != self.is_neg()) {
            None
        } else {
            Some(res)
        }
    }

    fn checked_neg(self) -> Option<Self> {
        let mut inv = [0u64; 8];
        for (dst, &src) in inv.iter_mut().zip(&self.0) {
            *dst = !src;
        }
        Self(inv).checked_add(Self::from_i128(1))
    }

    fn checked_sub(self, rhs: Self) -> Option<Self> {
        rhs.checked_neg().and_then(|neg_r| self.checked_add(neg_r))
    }

    fn shl_bits(mut self, shift: u32) -> Self {
        for _ in 0..shift {
            let mut carry = 0u64;
            for w in &mut self.0 {
                let next = *w >> 63;
                *w = (*w << 1) | carry;
                carry = next;
            }
        }
        self
    }

    fn shr_bits(mut self, shift: u32) -> Self {
        for _ in 0..shift {
            let mut carry = self.0[7] & (1u64 << 63);
            for w in self.0.iter_mut().rev() {
                let next = (*w & 1) << 63;
                *w = (*w >> 1) | carry;
                carry = next;
            }
        }
        self
    }

    fn checked_shl(self, rhs: Self) -> Option<Self> {
        if rhs.is_neg() {
            return None;
        }
        let zero = Self::from_i128(0);
        if self == zero {
            return Some(zero);
        }
        let shift = u32::try_from(rhs.to_i128()?).ok()?;
        if shift >= 512 {
            return None;
        }
        let shifted = self.shl_bits(shift);
        if shifted.shr_bits(shift) == self {
            Some(shifted)
        } else {
            None
        }
    }

    fn checked_shr(self, rhs: Self) -> Option<Self> {
        if rhs.is_neg() {
            return None;
        }
        let shift = rhs
            .to_i128()
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(512)
            .min(512);
        Some(self.shr_bits(shift))
    }

    fn checked_mul(self, rhs: Self) -> Option<Self> {
        let neg = self.is_neg() ^ rhs.is_neg();
        let mut a = if self.is_neg() { self.checked_neg()? } else { self };
        let mut b = if rhs.is_neg() { rhs.checked_neg()? } else { rhs };
        let zero = Self::from_i128(0);
        let mut acc = zero;
        while b != zero {
            if (b.0[0] & 1) == 1 {
                acc = acc.checked_add(a)?;
            }
            b = b.shr_bits(1);
            if b != zero {
                let next_a = a.shl_bits(1);
                if next_a.shr_bits(1) != a {
                    return None;
                }
                a = next_a;
            }
        }
        if neg { acc.checked_neg() } else { Some(acc) }
    }

    fn div_rem_pos(a: Self, b: Self) -> (Self, Self) {
        let mut q = Self::from_i128(0);
        let mut r = Self::from_i128(0);
        for i in (0..512).rev() {
            r = r.shl_bits(1);
            r.0[0] |= (a.0[i / 64] >> (i % 64)) & 1;
            if let Some(sub) = r.checked_sub(b) {
                if !sub.is_neg() {
                    r = sub;
                    q.0[i / 64] |= 1u64 << (i % 64);
                }
            }
        }
        (q, r)
    }

    fn checked_div_rem(self, rhs: Self) -> Option<(Self, Self)> {
        let zero = Self::from_i128(0);
        if rhs == zero {
            return None;
        }
        let a = if self.is_neg() { self.checked_neg()? } else { self };
        let b = if rhs.is_neg() { rhs.checked_neg()? } else { rhs };
        let (q, r) = Self::div_rem_pos(a, b);
        let q = if self.is_neg() ^ rhs.is_neg() { q.checked_neg()? } else { q };
        let r = if self.is_neg() { r.checked_neg()? } else { r };
        Some((q, r))
    }

    fn bit_op(self, rhs: Self, f: fn(u64, u64) -> u64) -> Self {
        let mut out = [0u64; 8];
        for i in 0..8 {
            out[i] = f(self.0[i], rhs.0[i]);
        }
        Self(out)
    }

    fn truncate_to_width(self, bits: u32, signed: bool) -> i128 {
        let low = (self.0[0] as u128) as i128;
        let len = 1i128 << bits;
        let masked = low & (len - 1);
        if signed && (masked >> (bits - 1)) & 1 == 1 {
            masked - len
        } else {
            masked
        }
    }
}

fn const_int_expr(expr: &hy_syntax::Expr) -> Option<WideInt> {
    use hy_syntax::{BinOp, Expr, UnOp};
    match expr {
        Expr::Paren { inner, .. } => const_int_expr(inner),
        Expr::Unary {
            op: UnOp::Neg,
            operand,
            ..
        } => const_int_expr(operand)?.checked_neg(),
        Expr::Lit {
            kind: LitKind::Int(t),
            ..
        } => parse_int(t).map(WideInt::from_i128),
        Expr::Lit {
            kind: LitKind::Char(c),
            ..
        } => Some(WideInt::from_i128(*c as u32 as i128)),
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = const_int_expr(lhs)?;
            let r = const_int_expr(rhs)?;
            match op {
                BinOp::Add => l.checked_add(r),
                BinOp::Sub => l.checked_sub(r),
                BinOp::Mul => l.checked_mul(r),
                BinOp::Div => l.checked_div_rem(r).map(|(q, _)| q),
                BinOp::Mod => l.checked_div_rem(r).map(|(_, rem)| rem),
                BinOp::BitAnd => Some(l.bit_op(r, |a, b| a & b)),
                BinOp::BitOr => Some(l.bit_op(r, |a, b| a | b)),
                BinOp::BitXor => Some(l.bit_op(r, |a, b| a ^ b)),
                BinOp::Shl => l.checked_shl(r),
                BinOp::Shr => l.checked_shr(r),
                _ => None,
            }
        }
        _ => None,
    }
}

fn has_out_of_range_subexpr(expr: &hy_syntax::Expr, bits: u32, signed: bool) -> bool {
    use hy_syntax::Expr;
    let Some(v) = const_int_expr(expr) else {
        return false;
    };
    let folded = v.truncate_to_width(bits, signed);
    if v.to_i128() != Some(folded) {
        return true;
    }
    match expr {
        Expr::Paren { inner, .. } | Expr::Unary { operand: inner, .. } => {
            has_out_of_range_subexpr(inner, bits, signed)
        }
        Expr::Binary { lhs, rhs, .. } => {
            has_out_of_range_subexpr(lhs, bits, signed)
                || has_out_of_range_subexpr(rhs, bits, signed)
        }
        _ => false,
    }
}

/// Fold a constant conversion operand into the target's representation.
///
/// Returns `None` when nothing should change: not an integer constant, an
/// unknown width, or a value (and all subexpressions) that already fits.
pub fn fold_constant(operand: &hy_syntax::Expr, spec_name: &str) -> Option<i128> {
    let (bits, signed) = width_of(spec_name)?;
    if bits > 64 {
        return None;
    }
    let v = const_int_expr(operand)?;
    let folded = v.truncate_to_width(bits, signed);

    // Only rewrite what Go would refuse; a fitting value whose sub-operands
    // also fit in the target width stays verbatim.
    if v.to_i128() == Some(folded) && !has_out_of_range_subexpr(operand, bits, signed) {
        None
    } else {
        Some(folded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The first argument of a one-argument call expression - the shape the
    /// emitter hands to `fold_constant` (`u8(300)` -> the `300` literal).
    fn operand_of(src: &str) -> hy_syntax::Expr {
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        let f = e.fns().next().unwrap();
        let stmt = &f.body.as_ref().unwrap().stmts[0];
        let call = match stmt {
            hy_syntax::Stmt::Local(l) => &l.values[0],
            _ => panic!("expected a local"),
        };
        match call {
            hy_syntax::Expr::Call { args, .. } => {
                assert_eq!(args.len(), 1, "one-arg conversion");
                args[0].value.clone()
            }
            other => panic!("expected a call, got {:?}", other),
        }
    }

    #[test]
    fn spec_example_folds_to_44() {
        // The SPEC's own claim: `b := u8(300)  // expect: 44`.
        let operand = operand_of("fn main() { b := u8(300) }");
        assert_eq!(fold_constant(&operand, "u8"), Some(44));
    }

    #[test]
    fn fitting_value_is_untouched() {
        let operand = operand_of("fn main() { b := u8(7) }");
        assert_eq!(fold_constant(&operand, "u8"), None, "7 fits, must not fold");
    }

    #[test]
    fn hex_and_separators_parse() {
        assert_eq!(parse_int("0x12C"), Some(300));
        assert_eq!(parse_int("0b1010"), Some(10));
        assert_eq!(parse_int("1_000"), Some(1000));
        assert_eq!(parse_int("0o17"), Some(15));
    }

    #[test]
    fn signed_target_truncates_twos_complement() {
        // 200 does not fit i8; two's complement in 8 bits is -56.
        let operand = operand_of("fn main() { b := i8(200) }");
        assert_eq!(fold_constant(&operand, "i8"), Some(-56));
    }

    #[test]
    fn float_targets_are_not_width_folded() {
        assert_eq!(width_of("f32"), None);
        assert_eq!(width_of("f64"), None);
    }

    #[test]
    fn go_spelling_conversion_folds_like_spec_spelling() {
        // issue #228 C1: `int8(300)` is the same Hybrid type as `i8(300)`
        // (the checker's fixed_width_int_range equates them) and must take
        // the same SPEC X4 fold instead of emitting `int8(300)` verbatim
        // (go: `constant 300 overflows int8`).
        let operand = operand_of("fn main() { b := int8(300) }");
        assert_eq!(fold_constant(&operand, "int8"), Some(44));
        let operand = operand_of("fn main() { b := byte(300) }");
        assert_eq!(fold_constant(&operand, "byte"), Some(44));
        let operand = operand_of("fn main() { b := rune(99999999999) }");
        assert_eq!(fold_constant(&operand, "rune"), Some(1215752191));
    }

    #[test]
    fn negated_and_parenthesized_operands_fold() {
        // issue #228 C3: Go rejects `uint8(-1)` exactly like `uint8(300)`,
        // so the sign has to reach the fold — `-1` truncates to 255.
        let operand = operand_of("fn main() { b := uint8(-1) }");
        assert_eq!(fold_constant(&operand, "uint8"), Some(255));
        let operand = operand_of("fn main() { b := i8(-300) }");
        assert_eq!(fold_constant(&operand, "i8"), Some(-44));
        let operand = operand_of("fn main() { b := int8((300)) }");
        assert_eq!(fold_constant(&operand, "int8"), Some(44));
    }
}
