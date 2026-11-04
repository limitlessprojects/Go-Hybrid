//! issue_3589_complex_ordered_comparison.rs: regression for #3589.
//!
//! Under Go and Hybrid specifications, ordered comparisons (`<`, `<=`, `>`, `>=`)
//! are not defined on complex types (`complex64`, `complex128`, and `complex(...)`).
//! In Go, attempting ordered comparisons on complex values fails `go build` with
//! `operator < not defined on complex128` (Row 13 BUILDABLE).
//!
//! Equality operators (`==`, `!=`) and arithmetic operators (`+`, `-`, `*`, `/`)
//! on complex numbers remain valid and cleanly accepted.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_inferred_complex_greater_and_le_rejects() {
    let src = r#"
fn main() {
    c := complex(1.0, 2.0)
    d := complex(1.0, 3.0)
    fmt.println(c > d)
    fmt.println(c <= d)
}
"#;
    rejects(src);
}

#[test]
fn test_explicit_complex128_less_than_rejects() {
    let src = r#"
fn main() {
    var c complex128 = complex(1.0, 2.0)
    fmt.println(c < c)
}
"#;
    rejects(src);
}

#[test]
fn test_complex64_ge_rejects() {
    let src = r#"
fn main() {
    var c complex64
    var d complex64
    fmt.println(c >= d)
}
"#;
    rejects(src);
}

#[test]
fn test_direct_complex_call_comparison_rejects() {
    let src = r#"
fn main() {
    _ = complex(1.0, 2.0) < complex(2.0, 3.0)
}
"#;
    rejects(src);
}

#[test]
fn test_complex_with_untyped_float_ordered_rejects() {
    let src = r#"
fn main() {
    c := complex(1.0, 2.0)
    _ = c < 1.0
}
"#;
    rejects(src);
}

#[test]
fn test_complex_equality_and_inequality_accepted() {
    let src = r#"
fn main() {
    c := complex(1.0, 2.0)
    d := complex(1.0, 3.0)
    fmt.println(c == d, c != d)
}
"#;
    accepts(src);
}

#[test]
fn test_complex_arithmetic_accepted() {
    let src = r#"
fn main() {
    c := complex(1.0, 2.0)
    d := complex(1.0, 3.0)
    sum := c + d
    diff := c - d
    prod := c * d
    quot := c / d
    fmt.println(sum, diff, prod, quot)
}
"#;
    accepts(src);
}

#[test]
fn test_real_and_imag_ordered_comparison_accepted() {
    let src = r#"
fn main() {
    c := complex(1.0, 2.0)
    _ = real(c) < imag(c)
}
"#;
    accepts(src);
}
