//! channel_send_soundness.rs — regression tests for channel send statement operand soundness (fixes #2691).
//!
//! Validates:
//! 1. `ch <- i` sending typed `int` variable into `chan float64` rejected with HYB-E1002.
//! 2. `ch <- f` sending typed `float64` variable into `chan int` rejected with HYB-E1002.
//! 3. Select send arm sending typed `int` variable into `chan float64` rejected with HYB-E1002.
//! 4. Untyped integer constants into `chan float64` accepted cleanly.
//! 5. Matching typed variable sends accepted cleanly.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_send_typed_int_into_float_chan_rejected() {
    let src = r#"
fn main() {
    var ch chan float64
    var i int = 42
    ch <- i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_send_typed_float_into_int_chan_rejected() {
    let src = r#"
fn main() {
    var ch chan int
    var f float64 = 3.14
    ch <- f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_select_send_typed_int_into_float_chan_rejected() {
    let src = r#"
fn main() {
    var ch chan float64
    var i int = 42
    select {
    ch <- i:
    default:
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_send_untyped_int_const_into_float_chan_accepted() {
    let src = r#"
fn main() {
    var ch chan float64
    ch <- 1
    const c = 10
    ch <- c
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}

#[test]
fn test_send_matching_types_accepted() {
    let src = r#"
fn main() {
    var ch1 chan int
    var i int = 42
    ch1 <- i

    var ch2 chan float64
    var f float64 = 3.14
    ch2 <- f
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}
