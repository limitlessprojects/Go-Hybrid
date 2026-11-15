//! issue_2700_word_sized_const_expr_overflow.rs — tests for Issue #2700 (Row 13 BUILDABLE).
//!
//! A conversion to a word-sized integer type (`int`, `uint`, `uintptr`) whose operand
//! is an out-of-range constant expression must reject at check time with HYB-E6006.
//! Constant expressions that fit within the target range must pass.

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
fn word_sized_const_expr_overflow_rejected_e6006() {
    let cases = [
        (
            "int_shl_63_in_call",
            r#"
fn main() {
    fmt.println(int(1 << 63))
}
"#,
        ),
        (
            "int_shl_63_in_assign",
            r#"
fn main() {
    x := int(1 << 63)
    _ = x
}
"#,
        ),
        (
            "uint_shl_64_in_assign",
            r#"
fn main() {
    x := uint(1 << 64)
    _ = x
}
"#,
        ),
        (
            "int_max_plus_one",
            r#"
fn main() {
    x := int(9223372036854775807 + 1)
    _ = x
}
"#,
        ),
        (
            "int_underflow_expr",
            r#"
fn main() {
    x := int(-(1 << 63) - 1)
    _ = x
}
"#,
        ),
        (
            "uintptr_shl_64_in_assign",
            r#"
fn main() {
    x := uintptr(1 << 64)
    _ = x
}
"#,
        ),
        (
            "uint_shl_64_in_call",
            r#"
fn main() {
    fmt.println(uint(1 << 64))
}
"#,
        ),
        (
            "uintptr_shl_64_in_call",
            r#"
fn main() {
    fmt.println(uintptr(1 << 64))
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
            codes.contains(&"HYB-E6006".to_string()),
            "case {name} should emit HYB-E6006, got: {codes:?}"
        );
    }
}

#[test]
fn word_sized_const_expr_within_range_accepted() {
    let cases = [
        (
            "int_shl_62",
            r#"
fn main() {
    fmt.println(int(1 << 62))
}
"#,
        ),
        (
            "uint_shl_63",
            r#"
fn main() {
    fmt.println(uint(1 << 63))
}
"#,
        ),
        (
            "int_add_in_range",
            r#"
fn main() {
    fmt.println(int(100 + 20))
}
"#,
        ),
        (
            "uintptr_shl_63",
            r#"
fn main() {
    fmt.println(uintptr(1 << 63))
}
"#,
        ),
        (
            "int_min_boundary",
            r#"
fn main() {
    fmt.println(int(-(1 << 63)))
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
