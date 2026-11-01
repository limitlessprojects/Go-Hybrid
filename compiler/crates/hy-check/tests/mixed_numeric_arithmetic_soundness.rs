//! mixed_numeric_arithmetic_soundness.rs — regression tests for Issue #2731:
//! verification that mixed concrete integer and float operands in binary arithmetic (`+`, `-`, `*`, `/`),
//! ordered comparisons (`<`, `<=`, `>`, `>=`), and compound assignments are rejected with HYB-E1002
//! under Row 13 (BUILDABLE), adhering to Go spec §Arithmetic operators and §Comparison operators.

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
fn mismatched_binary_arithmetic_rejected() {
    let cases = [
        ("i + f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i + f\n}"),
        ("i - f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i - f\n}"),
        ("i * f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i * f\n}"),
        ("i / f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i / f\n}"),
        ("f + i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f + i\n}"),
        ("f - i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f - i\n}"),
        ("f * i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f * i\n}"),
        ("f / i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f / i\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "missing HYB-E1002 for {}", desc);
    }
}

#[test]
fn mismatched_ordered_comparisons_rejected() {
    let cases = [
        ("i < f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i < f\n}"),
        ("i <= f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i <= f\n}"),
        ("i > f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i > f\n}"),
        ("i >= f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = i >= f\n}"),
        ("f < i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f < i\n}"),
        ("f >= i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    _ = f >= i\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "missing HYB-E1002 for {}", desc);
    }
}

#[test]
fn mismatched_compound_assignments_rejected() {
    let cases = [
        ("f += i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    f += i\n}"),
        ("f -= i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    f -= i\n}"),
        ("f *= i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    f *= i\n}"),
        ("f /= i", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    f /= i\n}"),
        ("i += f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    i += f\n}"),
        ("i -= f", "fn main() {\n    var i int = 1\n    var f float64 = 2.0\n    i -= f\n}"),
    ];

    for (desc, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", desc);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "missing HYB-E1002 for {}", desc);
    }
}

#[test]
fn valid_numeric_operations_stay_accepted() {
    let src = r#"
fn main() {
    // 1. Same-type operations
    var i1 int = 1
    var i2 int = 2
    _ = i1 + i2
    _ = i1 - i2
    _ = i1 * i2
    _ = i1 / i2
    _ = i1 < i2

    var f1 float64 = 1.0
    var f2 float64 = 2.0
    _ = f1 + f2
    _ = f1 - f2
    _ = f1 * f2
    _ = f1 / f2
    _ = f1 < f2

    // 2. Untyped constants adapt cleanly
    _ = f1 + 1
    _ = 1 + f1
    _ = f1 < 10
    f1 += 1
    f1 -= 2

    _ = i1 + 5
    i1 += 1

    // 3. String operations
    var s1 string = "hello"
    var s2 string = "world"
    _ = s1 + s2
    _ = s1 < s2
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", res.diagnostics);
}
