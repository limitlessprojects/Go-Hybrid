//! return_and_send_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables returned from float64 functions and sent to chan float64,
//! while preserving untyped integer constants acceptance.

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
fn single_return_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_ret() float64 {
    var i int = 1
    return i
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
        res.diagnostics.iter().any(|d| d.message.contains("return statement")),
        "diagnostic should mention return statement: {:?}",
        res.diagnostics
    );
}

#[test]
fn multi_return_typed_int_to_float_is_rejected() {
    let src = r#"
fn test_multi_ret() (float64, string) {
    var i int = 1
    return i, "hello"
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
        res.diagnostics.iter().any(|d| d.message.contains("return statement")),
        "diagnostic should mention return statement: {:?}",
        res.diagnostics
    );
}

#[test]
fn channel_send_typed_int_to_float_chan_is_rejected() {
    let src = r#"
fn test_chan_send() {
    var ch chan float64
    var i int = 1
    ch <- i
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in send")),
        "diagnostic should mention send mismatch: {:?}",
        res.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_return_and_send_are_accepted() {
    let src = r#"
const k = 2

fn test_ret_const1() float64 {
    return 1
}

fn test_ret_const2() float64 {
    return k
}

fn test_ret_const3() float64 {
    return k + 1
}

fn test_chan_const() {
    var ch chan float64
    ch <- 1
    ch <- k
    ch <- k + 1
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal in return and send: {:?}",
        res.diagnostics
    );
}
