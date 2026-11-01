//! fn_call_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables passed to float64 function and method parameters,
//! while preserving untyped integer constants adaptation.

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
fn fn_call_typed_int_to_float_param_is_rejected() {
    let src = r#"
fn set_rate(rate float64) {
}

fn test_fn_call() {
    var i int = 42
    set_rate(i)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("argument to `set_rate`")),
        "diagnostic should mention argument to set_rate: {:?}",
        res.diagnostics
    );
}

#[test]
fn method_call_typed_int_to_float_param_is_rejected() {
    let src = r#"
struct calc {
}

fn (c calc) set_rate(rate float64) {
}

fn test_method_call() {
    var c calc
    var i int = 42
    c.set_rate(i)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("argument to `calc.set_rate`")),
        "diagnostic should mention argument to calc.set_rate: {:?}",
        res.diagnostics
    );
}

#[test]
fn fn_and_method_call_untyped_constants_stay_accepted() {
    let src = r#"
const k = 2

struct calc {
}

fn set_rate(rate float64) {
}

fn (c calc) set_rate(rate float64) {
}

fn test_const_call() {
    var c calc
    set_rate(1)
    set_rate(k)
    set_rate(k + 1)
    c.set_rate(1)
    c.set_rate(k)
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped constants should be legal in calls to float64 params: {:?}",
        res.diagnostics
    );
}
