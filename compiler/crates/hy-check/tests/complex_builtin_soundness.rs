//! complex_builtin_soundness.rs — regression tests for Issue #2896.
//!
//! Built-in functions `complex`, `real`, and `imag` enforce strict operand type rules:
//! - `complex(r, i)` requires both operands to be floating-point types (`float32` or `float64`)
//!   with identical scalar types.
//! - `real(c)` and `imag(c)` require the operand `c` to be of complex type (`complex64` or `complex128`).

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
fn complex_int_operands_rejected() {
    let src = r#"
fn test_complex() {
    var r int32 = 1
    var i int32 = 2
    c := complex(r, i)
    _ = c
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for int operands in complex, got {:?}",
        res.diagnostics
    );
}

#[test]
fn complex_string_operands_rejected() {
    let src = r#"
fn test_complex() {
    var r string = "1"
    var i float64 = 2.0
    c := complex(r, i)
    _ = c
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for string operand in complex, got {:?}",
        res.diagnostics
    );
}

#[test]
fn complex_mismatched_scalars_rejected() {
    let src = r#"
fn test_complex() {
    var r float32 = 1.0
    var i float64 = 2.0
    c := complex(r, i)
    _ = c
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for float32/float64 mismatch in complex, got {:?}",
        res.diagnostics
    );
}

#[test]
fn real_non_complex_float_rejected() {
    let src = r#"
fn test_real() {
    var f float64 = 1.0
    x := real(f)
    _ = x
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for non-complex operand in real, got {:?}",
        res.diagnostics
    );
}

#[test]
fn real_non_complex_int_rejected() {
    let src = r#"
fn test_real() {
    var n int32 = 1
    x := real(n)
    _ = x
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for int operand in real, got {:?}",
        res.diagnostics
    );
}

#[test]
fn imag_non_complex_string_rejected() {
    let src = r#"
fn test_imag() {
    var s string = "abc"
    x := imag(s)
    _ = x
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for string operand in imag, got {:?}",
        res.diagnostics
    );
}

#[test]
fn complex_real_imag_valid_accepted() {
    let src = r#"
fn test_valid() {
    var r32 float32 = 1.0
    var i32 float32 = 2.0
    c1 := complex(r32, i32)
    _ = c1

    var r64 float64 = 1.0
    var i64 float64 = 2.0
    c2 := complex(r64, i64)
    _ = c2

    c3 := complex(r32, 2.0)
    _ = c3

    c4 := complex(1.0, 2.0)
    _ = c4

    c5 := complex(1, 2)
    _ = c5

    var c64 complex64
    x1 := real(c64)
    y1 := imag(c64)
    _ = x1
    _ = y1

    var c128 complex128
    x2 := real(c128)
    y2 := imag(c128)
    _ = x2
    _ = y2

    x3 := real(complex(1.0, 2.0))
    _ = x3
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "diagnostics: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty(), "expected zero diagnostics, got {:?}", res.diagnostics);
}
