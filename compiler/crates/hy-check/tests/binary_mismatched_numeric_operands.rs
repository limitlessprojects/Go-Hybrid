//! binary_mismatched_numeric_operands.rs — regression tests for binary arithmetic
//! and comparison operators mismatched numeric operand types soundness under
//! Row 13 (BUILDABLE) and HYB-E1002 (issue #2504).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got verdict {:?}, diags {:?}",
        what,
        r.verdict,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", what);
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got errors: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn mismatched_numeric_arithmetic_operands_are_rejected() {
    let src_add = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x + f\n}\n";
    assert_rejects_e1002(src_add, "x + f");

    let src_sub = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x - f\n}\n";
    assert_rejects_e1002(src_sub, "x - f");

    let src_mul = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x * f\n}\n";
    assert_rejects_e1002(src_mul, "x * f");

    let src_div = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x / f\n}\n";
    assert_rejects_e1002(src_div, "x / f");
}

#[test]
fn mismatched_numeric_comparison_operands_are_rejected() {
    let src_lt = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x < f\n}\n";
    assert_rejects_e1002(src_lt, "x < f");

    let src_le = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x <= f\n}\n";
    assert_rejects_e1002(src_le, "x <= f");

    let src_gt = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x > f\n}\n";
    assert_rejects_e1002(src_gt, "x > f");

    let src_ge = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x >= f\n}\n";
    assert_rejects_e1002(src_ge, "x >= f");
}

#[test]
fn untyped_constants_and_matched_types_stay_accepted() {
    let src = "fn main() {\n    var x int = 1\n    var f float64 = 2.0\n    _ = x + 1\n    _ = f + 2.0\n    _ = f + 1\n    _ = 1 + f\n    _ = x < 10\n    _ = f >= 0.0\n    var y int = 2\n    _ = x + y\n    _ = x < y\n}\n";
    assert_ok(src, "untyped constants and matching numeric operands");
}
