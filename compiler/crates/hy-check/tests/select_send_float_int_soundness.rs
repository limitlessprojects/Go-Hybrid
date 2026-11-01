//! select_send_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables/constants sent to chan float64 inside select
//! statements and send statements, while preserving untyped integer constants.
//!
//! Regression test for Issue #2577.

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
fn select_send_typed_int_variable_to_chan_float_is_rejected() {
    let src = r#"
fn test_select_send_var() {
    var ch chan float64
    var i int = 1
    select {
    ch <- i:
    default:
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
    assert!(
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (variable of type `int`) as `float64` value in send")),
        "diagnostic should mention sending int variable to float channel: {:?}",
        res.diagnostics
    );
}

#[test]
fn stmt_send_typed_int_variable_to_chan_float_is_rejected() {
    let src = r#"
fn test_stmt_send_var() {
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
}

#[test]
fn stmt_send_typed_float_variable_to_chan_int_is_rejected() {
    let src = r#"
fn test_stmt_send_float_to_chan_int() {
    var ch chan int
    var f float64 = 3.14
    ch <- f
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
fn select_send_typed_float_variable_to_chan_int_is_rejected() {
    let src = r#"
fn test_select_send_float_to_chan_int() {
    var ch chan int
    var f float64 = 3.14
    select {
    ch <- f:
    default:
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
fn select_send_typed_int_const_to_chan_float_is_rejected() {
    let src = r#"
const k int = 1

fn test_select_send_typed_const() {
    var ch chan float64
    select {
    ch <- k:
    default:
    }
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "typed int const should be rejected for chan float64: {:?}",
        res.diagnostics
    );
}

#[test]
fn select_and_stmt_send_untyped_constants_are_accepted() {
    let src = r#"
const k = 1

fn test_send_untyped_consts() {
    var ch chan float64
    select {
    ch <- 1:
    ch <- k:
    ch <- k + 2:
    ch <- -1:
    default:
    }
    ch <- 1
    ch <- k
    ch <- k * 3
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal for chan float64: {:?}",
        res.diagnostics
    );
}

#[test]
fn send_inside_managed_block_is_permitted() {
    let src = r#"
fn test_managed_send() {
    counts := map[string]int{}
    ch := make(chan int, 1)
    managed("test hatch", "mutex") {
        ch <- counts
    }
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "send inside managed block should honor the hatch: {:?}",
        res.diagnostics
    );
}
