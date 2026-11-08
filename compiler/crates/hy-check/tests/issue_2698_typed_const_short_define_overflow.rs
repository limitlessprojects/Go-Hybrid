//! issue_2698_typed_const_short_define_overflow.rs — tests for Issue #2698.
//!
//! A short variable definition (`x := expr`) whose initializer is a typed constant
//! expression (e.g. `u64(1 << 63)`, `u64(18446744073709551615)`) gives `x` the type
//! of that expression. It is not an untyped constant and must not be rejected with
//! HYB-E6002 "overflows default type int".
//!
//! Untyped constant expressions outside `int` range and genuinely folded typed overflows
//! (such as `u64(1) << 64` or `u8(255) + 1` under #725) must still be rejected.

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn typed_const_short_defines_accepted() {
    let cases = [
        (
            "u64_shl_63",
            r#"
fn main() {
    x := u64(1 << 63)
    _ = x
}
"#,
        ),
        (
            "u64_max_dec",
            r#"
fn main() {
    x := u64(18446744073709551615)
    _ = x
}
"#,
        ),
        (
            "u64_max_hex",
            r#"
fn main() {
    x := u64(0xFFFFFFFFFFFFFFFF)
    _ = x
}
"#,
        ),
        (
            "uint_max_dec",
            r#"
fn main() {
    x := uint(18446744073709551615)
    _ = x
}
"#,
        ),
        (
            "u64_1_shl_63",
            r#"
fn main() {
    x := u64(1) << 63
    _ = x
}
"#,
        ),
        (
            "u64_shl_62_mul_2",
            r#"
fn main() {
    x := u64(1 << 62) * 2
    _ = x
}
"#,
        ),
        (
            "u64_paren_shl_63",
            r#"
fn main() {
    x := (u64(1 << 63))
    _ = x
}
"#,
        ),
        (
            "u64_fnv_seed",
            r#"
fn main() {
    x := u64(0xcbf29ce484222325)
    _ = x
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {name} should accept: {src}, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn untyped_and_folded_typed_overflow_rejected_e6002() {
    let cases = [
        (
            "untyped_shl_63",
            r#"
fn main() {
    x := 1 << 63
    _ = x
}
"#,
        ),
        (
            "untyped_overflow_lit",
            r#"
fn main() {
    x := 9223372036854775808
    _ = x
}
"#,
        ),
        (
            "typed_u64_overflow_shl_64",
            r#"
fn main() {
    x := u64(1) << 64
    _ = x
}
"#,
        ),
        (
            "typed_u8_overflow_add_1",
            r#"
fn main() {
    x := u8(255) + 1
    _ = x
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should reject: {src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E6002".to_string()),
            "case {name} should emit HYB-E6002, got: {codes:?}"
        );
    }
}
