//! const_conversion.rs — regression for issue #228 family C: constant
//! misuse in `Type(const)` conversion calls used to pass `hyb check`
//! (0 diagnostics) and then fail `go build` on the emitted program
//! (`constant 300 overflows int8`, `cannot convert "abc" to type int`,
//! `invalid operation: cannot convert 3 to type []int`, …).
//!
//! Sub-64 integer overflow is deliberately NOT rejected: SPEC-LANGUAGE
//! line 1991's executable claim `u8(300) == 44` makes the emitter fold the
//! two's-complement result, so those forms stay accepted on purpose.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6006"),
        "expected HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6006"),
        "must NOT fire HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn int64_sized_integer_overflows_reject() {
    // `int`/`int64`/`uint64` skip the emitter's X4 fold (bits >= 64), so an
    // out-of-range constant passes through verbatim and Go rejects it.
    rejects_with_e6006("fn main() { x := int(10000000000000000000) _ = x }");
    rejects_with_e6006("fn main() { x := int64(99999999999999999999) _ = x }");
    rejects_with_e6006("fn main() { x := uint64(18446744073709551616) _ = x }");
    rejects_with_e6006(
        "fn main() { x := int(999999999999999999999999999999999999999999999999) _ = x }",
    );
}

#[test]
fn fractional_or_out_of_range_float_to_int_rejects() {
    rejects_with_e6006("fn main() { x := int8(2.5) _ = x }");
    rejects_with_e6006("fn main() { x := uint8(-2.0) _ = x }");
    rejects_with_e6006("fn main() { x := int8(300.0) _ = x }");
    rejects_with_e6006("fn main() { x := int(2.5) _ = x }");
}

#[test]
fn non_numeric_constants_to_numeric_targets_reject() {
    rejects_with_e6006("fn main() { x := int(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := int8(nil) _ = x }");
    rejects_with_e6006("fn main() { x := int8(true) _ = x }");
    rejects_with_e6006("fn main() { x := uint8(false) _ = x }");
}

#[test]
fn string_target_rejects_float_bool_nil() {
    // Go allows string(65), string(\"…\"), string('A') — NOT string(2.5),
    // string(true), string(nil).
    rejects_with_e6006("fn main() { x := string(2.5) _ = x }");
    rejects_with_e6006("fn main() { x := string(true) _ = x }");
    rejects_with_e6006("fn main() { x := string(nil) _ = x }");
}

#[test]
fn float_targets_reject_nonfinite_and_f32_overflow() {
    rejects_with_e6006("fn main() { x := float32(1e40) _ = x }");
    rejects_with_e6006("fn main() { x := f32(1e40) _ = x }");
    // Past-i128 integer literal: the f64 reading still has to respect
    // f32's max (`float32(10^41)` is `cannot convert ... to type
    // float32` in Go) while f64 keeps it.
    rejects_with_e6006(&format!(
        "fn main() {{ x := f32(1{}) _ = x }}",
        "0".repeat(41)
    ));
    accepts_without_e6006(&format!(
        "fn main() {{ x := float64(1{}) _ = x }}",
        "0".repeat(41)
    ));
    rejects_with_e6006("fn main() { x := float64(\"abc\") _ = x }");
}

#[test]
fn string_conversion_big_integer_uses_the_2_pow_512_boundary() {
    // go1.25 verified: string(10^150) builds, string(2^512 - 1) builds,
    // string(2^512) and string(10^200) are `constant overflow`.
    accepts_without_e6006(&format!(
        "fn main() {{ x := string(1{}) _ = x }}",
        "0".repeat(150)
    ));
    rejects_with_e6006(&format!(
        "fn main() {{ x := string(1{}) _ = x }}",
        "0".repeat(200)
    ));
    rejects_with_e6006(&format!(
        "fn main() {{ x := string(-1{}) _ = x }}",
        "0".repeat(200)
    ));
}

#[test]
fn rune_out_of_range_for_byte_target_rejects() {
    // The fold only rewrites integer literals — `byte('Ā')` (256) would be
    // emitted verbatim and die as `constant 256 overflows byte`.
    rejects_with_e6006("fn main() { x := byte('\\u{100}') _ = x }");
    rejects_with_e6006("fn main() { x := int8('\\u{100}') _ = x }");
}

#[test]
fn slice_conversion_rejects_non_byte_rune_elements_and_non_string_args() {
    rejects_with_e6006("fn main() { x := []int(3) _ = x }");
    rejects_with_e6006("fn main() { x := []int(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := []string(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := []byte(2.5) _ = x }");
}

// ---------------------------------------------------------------- accept:

#[test]
fn sub64_integer_overflow_folds_and_stays_accepted() {
    // SPEC X4 / line 1991: `u8(300) == 44`. The same fold applies to the
    // Go spellings (`int8(300)`, `byte(300)`), so these ACCEPT here and
    // emit already-truncated literals.
    for src in [
        "fn main() { x := u8(300) _ = x }",
        "fn main() { x := int8(300) _ = x }",
        "fn main() { x := byte(300) _ = x }",
        "fn main() { x := uint8(-1) _ = x }",
        "fn main() { x := i8(-300) _ = x }",
        "fn main() { x := int8((300)) _ = x }",
        "fn main() { x := rune(99999999999) _ = x }",
    ] {
        accepts_without_e6006(src);
    }
}

#[test]
fn go_legal_conversions_stay_accepted() {
    for src in [
        "fn main() { x := string(65) _ = x }",
        "fn main() { x := string(-1) _ = x }",
        "fn main() { x := string(99999999999) _ = x }",
        "fn main() { x := string(\"hi\") _ = x }",
        "fn main() { x := string('A') _ = x }",
        "fn main() { x := int8(2.0) _ = x }",
        "fn main() { x := float64(2.5) _ = x }",
        "fn main() { x := f64(2.5) _ = x }",
        "fn main() { x := float64(1000000000000000000000000000000) _ = x }",
        "fn main() { x := int(300) _ = x }",
        "fn main() { x := byte('A') _ = x }",
        "fn main() { x := []byte(\"abc\") _ = x }",
        "fn main() { x := []rune(\"abc\") _ = x }",
    ] {
        accepts_without_e6006(src);
    }
}

#[test]
fn non_constant_operands_stay_accepted() {
    // Run-time conversions (SPEC X4) are out of scope — same literal-only
    // precedent as E6001/E6002 (#149/#148).
    accepts_without_e6006("fn main() { n := 300 x := int8(n) _ = x }");
    accepts_without_e6006("fn main() { s := \"abc\" x := int(s.len()) _ = x }");
    accepts_without_e6006("fn main() { s := \"abc\" x := []byte(s) _ = x }");
    accepts_without_e6006("fn main() { k := 5 x := int(k) _ = x }");
}

#[test]
fn closure_bodies_are_checked_deep() {
    // FnLit bodies are not flattened into Pass B — walk them deep, same as
    // the zero-divisor pass.
    rejects_with_e6006("fn main() { f := fn() { x := int(\"a\") _ = x } _ = f }");
    rejects_with_e6006("fn main() { defer int(\"a\") }");
}

#[test]
fn controls_untouched() {
    // E6002 ascription path unchanged; user calls / methods never fire
    // E6006.
    let r = check("fn main() { var x int8 = 300 _ = x }");
    let cs = error_codes(&r);
    assert!(cs.iter().any(|c| c == "HYB-E6002"), "got {:?}", cs);
    assert!(!cs.iter().any(|c| c == "HYB-E6006"), "got {:?}", cs);

    accepts_without_e6006("fn main() { x := foo(300) _ = x } fn foo(n int) int { return n }");
    accepts_without_e6006(
        "struct pair { n int } fn main() { p := pair{ n: 1 } x := int8(p.n) _ = x }",
    );
}

#[test]
fn array_conversion_rejects_constant_operands() {
    rejects_with_e6006("fn main() { x := [4]byte{}(\"text\") _ = x }");
    rejects_with_e6006("fn main() { x := [4]int{}(nil) _ = x }");
    rejects_with_e6006("fn main() { x := [4]int{}(123) _ = x }");
    rejects_with_e6006("fn main() { x := [4]int{}(true) _ = x }");
    rejects_with_e6006("fn main() { x := [4]int{}(1.5) _ = x }");
}

