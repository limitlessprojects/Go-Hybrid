//! select_send_float_int_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables sent to chan float64 in select send arms,
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
fn select_send_typed_int_to_float_chan_is_rejected() {
    let src = r#"
fn test_select_send() {
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
        res.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as `float64` value in send")),
        "diagnostic should mention send mismatch: {:?}",
        res.diagnostics
    );
}

#[test]
fn select_send_untyped_int_constants_are_accepted() {
    let src = r#"
const k = 2

fn test_select_const() {
    var ch chan float64
    select {
    ch <- 1:
    ch <- k:
    ch <- k + 1:
    default:
    }
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "untyped int constants should be legal in select send: {:?}",
        res.diagnostics
    );
}

#[test]
fn stmt_send_typed_int_to_float_chan_is_rejected() {
    let src = r#"
fn test_stmt_send() {
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
