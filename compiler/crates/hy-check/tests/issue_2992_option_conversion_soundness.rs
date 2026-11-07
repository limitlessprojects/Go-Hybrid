//! issue_2992_option_conversion_soundness.rs — regression tests for Issue #2992:
//! verifies that converting an option[T] value to a scalar or string type
//! is rejected with HYB-E1002 (fixes #2992, Row 13 BUILDABLE).

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

fn assert_reject_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for snippet:\n{src}\ngot Accept with diagnostics: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected error code HYB-E1002 for snippet:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn option_to_integer_types_conversion_rejected_with_e1002() {
    let int_targets = ["u8", "int", "i64", "rune", "uint32", "i16"];
    for target in int_targets {
        let src = format!(
            "fn main() {{\n    op := some(1)\n    _ = {}(op)\n}}\n",
            target
        );
        assert_reject_e1002(&src);
    }
}

#[test]
fn option_to_float_types_conversion_rejected_with_e1002() {
    let float_targets = ["float64", "float32", "f64", "f32"];
    for target in float_targets {
        let src = format!(
            "fn main() {{\n    op := some(1)\n    _ = {}(op)\n}}\n",
            target
        );
        assert_reject_e1002(&src);
    }

    let fn_call_src = "\
fn f() option[int] { return some(2) }
fn main() {
    _ = float64(f())
}
";
    assert_reject_e1002(fn_call_src);
}

#[test]
fn option_to_string_conversion_rejected_with_e1002() {
    let src = "\
fn main() {
    op := some(1)
    _ = string(op)
}
";
    assert_reject_e1002(src);
}

#[test]
fn option_conversion_inside_call_rejected_with_e1002() {
    let src = "\
fn take(n int) {}
fn main() {
    op := some(1)
    take(int(op))
}
";
    assert_reject_e1002(src);
}

#[test]
fn valid_option_operations_remain_accepted() {
    let src = "\
fn main() {
    op := some(1)
    _ = u8(op.unwrap())
    _ = float64(op.or(3))
    _ = any(op)
    var op2 option[int] = op
    _ = op2
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for valid option controls, got: {:?}",
        r.diagnostics
    );
}
