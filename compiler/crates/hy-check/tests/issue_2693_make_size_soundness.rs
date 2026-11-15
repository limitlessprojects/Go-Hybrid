//! issue_2693_make_size_soundness.rs — regression tests for builtin make
//! size argument integer type soundness under Row 13 (BUILDABLE) and HYB-E1002 (issue #2693).

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
fn make_slice_non_integer_length_rejected() {
    let src_float = r#"
fn main() {
    var f float64 = 1.5
    s := make([]int, f)
    _ = s
}
"#;
    let r1 = check(src_float);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when passing float64 length to make([]int, ...)"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_str = r#"
fn main() {
    var s string = "five"
    arr := make([]int, s)
    _ = arr
}
"#;
    let r2 = check(src_str);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when passing string length to make([]int, ...)"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_slice_non_integer_capacity_rejected() {
    let src = r#"
fn main() {
    var cap_val float64 = 10.0
    s := make([]int, 2, cap_val)
    _ = s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected rejection when passing float64 capacity to make([]int, ...)"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn make_chan_non_integer_buffer_size_rejected() {
    let src_float = r#"
fn main() {
    var ch_size float64 = 5.0
    ch := make(chan int, ch_size)
    _ = ch
}
"#;
    let r1 = check(src_float);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "expected rejection when passing float64 buffer size to make(chan int, ...)"
    );
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    let src_str = r#"
fn main() {
    var ch_size string = "ten"
    ch := make(chan int, ch_size)
    _ = ch
}
"#;
    let r2 = check(src_str);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "expected rejection when passing string buffer size to make(chan int, ...)"
    );
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_make_calls_accepted() {
    let src = r#"
fn main() {
    var n int = 10
    var c int = 20

    s1 := make([]int, n)
    _ = s1

    s2 := make([]int, n, c)
    _ = s2

    s3 := make([]int, 5, 15)
    _ = s3

    ch1 := make(chan int)
    _ = ch1

    ch2 := make(chan int, n)
    _ = ch2

    ch3 := make(chan int, 5)
    _ = ch3
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected valid make calls with integer sizes to accept cleanly"
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got: {:?}",
        r.diagnostics
    );
}
