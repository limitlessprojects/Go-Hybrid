//! mixed_numeric_equality_soundness.rs — regression tests for Issue #2719:
//! verification that equality comparison (`==`, `!=`) between typed integer and float operands
//! is rejected with HYB-E1002 under Row 13 (BUILDABLE), adhering to Go spec §Comparison operators.

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
fn local_int_and_float_equality_is_rejected() {
    // 1. int == float64
    let src1 = "fn main() {\n    var a int = 1\n    var b float64 = 2.0\n    _ = a == b\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. int != float64
    let src2 = "fn main() {\n    var a int = 1\n    var b float64 = 2.0\n    _ = a != b\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. float64 == int
    let src3 = "fn main() {\n    var b float64 = 2.0\n    var a int = 1\n    _ = b == a\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));

    // 4. float32 != int
    let src4 = "fn main() {\n    var b float32 = 2.0\n    var a int = 1\n    _ = b != a\n}";
    let r4 = check(src4);
    assert_eq!(r4.verdict, Verdict::Reject);
    assert!(error_codes(&r4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn fn_and_closure_param_int_and_float_equality_is_rejected() {
    // 1. Function parameters: a int, b float64
    let src1 = "fn eq_check(a int, b float64) bool {\n    return a == b\n}\nfn main() {}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. Closure parameters
    let src2 = "fn main() {\n    f := fn(x int, y float64) bool { return x != y }\n    _ = f\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn typed_const_int_and_float_equality_is_rejected() {
    // Typed constants cannot be compared across int and float types in Go
    let src = "fn main() {\n    const a int = 1\n    const b float64 = 2.0\n    _ = a == b\n}";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_comparisons_stay_accepted() {
    let src = r#"
fn main() {
    // 1. Same types
    var a int = 1
    var b int = 2
    _ = a == b
    _ = a != b

    var f1 float64 = 1.0
    var f2 float64 = 2.0
    _ = f1 == f2
    _ = f1 != f2

    // 2. Untyped integer constant adapting to float64
    _ = f1 == 1
    _ = 1 == f1
    _ = f1 != 2
    _ = 2 != f1
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
