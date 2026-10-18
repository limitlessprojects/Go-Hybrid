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
pub fn width_of(spec_name: &str) -> Option<(u32, bool)> {
    Some(match spec_name {
        "u8" => (8, false),
        "u16" => (16, false),
        "u32" => (32, false),
        "u64" => (64, false),
        "i8" => (8, true),
        "i16" => (16, true),
        "i32" => (32, true),
        "i64" => (64, true),
        _ => return None,
    })
}

/// Integer literal text -> value. Accepts the SPEC-LANGUAGE §LX6 forms:
/// decimal, `0x`/`0o`/`0b` prefixed, `_` digit separators.
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
    } else {
        t.parse::<i128>().ok()?
    };
    Some(if neg { -v } else { v })
}

fn const_int_expr(mut expr: &hy_syntax::Expr) -> Option<i128> {
    let mut neg = false;
    loop {
        match expr {
            hy_syntax::Expr::Paren { inner, .. } => {
                expr = inner;
            }
            hy_syntax::Expr::Unary {
                op: hy_syntax::UnOp::Neg,
                operand,
                ..
            } => {
                neg = !neg;
                expr = operand;
            }
            hy_syntax::Expr::Lit {
                kind: LitKind::Int(t),
                ..
            } => {
                let v = parse_int(t)?;
                return if neg { v.checked_neg() } else { Some(v) };
            }
            _ => return None,
        }
    }
}

/// Fold a constant conversion operand into the target's representation.
///
/// Returns `None` when nothing should change: not an integer literal, an
/// unknown width, or a value that already fits.
pub fn fold_constant(operand: &hy_syntax::Expr, spec_name: &str) -> Option<i128> {
    let (bits, signed) = width_of(spec_name)?;
    if bits > 64 {
        return None;
    }
    let v = const_int_expr(operand)?;

    let len = 1i128 << bits;
    let masked = v & (len - 1);
    let folded = if signed && (masked >> (bits - 1)) & 1 == 1 {
        masked - len
    } else {
        masked
    };

    // Only rewrite what Go would refuse; a fitting value stays verbatim.
    if v == folded {
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
}
