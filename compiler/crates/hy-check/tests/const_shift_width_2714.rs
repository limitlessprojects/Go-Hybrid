//! const_shift_width_2714.rs — issue #2714 (Row 13 BUILDABLE).
//!
//! A constant shift count that reaches or exceeds the bit width of a typed
//! integer operand is a compile error in Go (`a << 8` on an `int8` is
//! `invalid operation: a << 8 (shift count 8 >= 8)`), yet `hyb check` accepted
//! it with 0 diagnostics. `hy-check` must reject it with `HYB-E1002` while
//! leaving below-width shifts, untyped left operands, and 64-bit operands
//! accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn repro_from_issue_rejects_e1002() {
    let src = "\
fn main() {
    var a int8 = 1
    _ = a << 8
    a <<= 8

    var b int16 = 1
    _ = b << 16

    var c int32 = 1
    _ = c << 32

    var d int64 = 1
    _ = d << 64
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn shifts_at_width_across_types_reject_e1002() {
    let src = "\
fn main() {
    var i8v int8 = 1
    _ = i8v >> 8
    var u8v uint8 = 255
    _ = u8v << 8
    var i16v int16 = 1
    _ = i16v >> 16
    var i32v int32 = 1
    _ = i32v << 32
    var i64v int64 = 1
    _ = i64v >> 64
    var u64v uint64 = 1
    _ = u64v << 64
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn compound_shift_at_width_rejects_e1002() {
    let src = "\
fn main() {
    var a int8 = 1
    a >>= 8
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn below_width_and_untyped_shifts_stay_accepted() {
    let src = "\
fn main() {
    var a int8 = 1
    _ = a << 7
    _ = a >> 7
    a <<= 7
    _ = a
    var n int = 1
    _ = n << 8
    _ = 1 << 8
    _ = 1 << 64
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn constant_left_operand_is_not_a_shift_width_error() {
    // A typed-constant (or any constant) left operand is a *constant shift*:
    // Go folds it and range-checks the RESULT, which the typed-constant
    // overflow pass reports as its own code. The width rule must NOT add
    // HYB-E1002 on top (its own fixture owns the E6002 half).
    let src = "\
fn main() {
    a := u8(1) << 9
    _ = a
    b := i64(1) << 63
    _ = b
}
";
    let r = check(src);
    assert!(
        !error_codes(&r).contains(&"HYB-E1002".to_string()),
        "constant shift must not be a width error: {:?}",
        r.diagnostics
    );
}

