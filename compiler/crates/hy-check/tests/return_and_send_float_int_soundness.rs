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
fn select_send_typed_int_to_float_chan_is_rejected() {
    let src = r#"
fn test_select_send() {
    var ch chan float64
    var i int = 1
    select {
    ch <- i:
    }
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
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
    const local_k = 3
    ch <- 1
    ch <- k
    ch <- local_k
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

#[test]
fn typed_int_constants_in_return_and_send_are_rejected() {
    let src_ret_file = r#"
const k int = 1

fn test_ret_typed_const() float64 {
    return k
}
"#;
    let res1 = check(src_ret_file);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(
        error_codes(&res1).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for typed const in return, got {:?}",
        res1.diagnostics
    );

    let src_send_file = r#"
const k int = 1

fn test_send_typed_const() {
    var ch chan float64
    ch <- k
}
"#;
    let res2 = check(src_send_file);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(
        error_codes(&res2).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for typed const in send, got {:?}",
        res2.diagnostics
    );

    let src_local = r#"
fn test_local_typed_const() float64 {
    const k int = 1
    return k
}
"#;
    let res3 = check(src_local);
    assert_eq!(res3.verdict, Verdict::Reject);
    assert!(
        error_codes(&res3).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for local typed const in return, got {:?}",
        res3.diagnostics
    );
}
