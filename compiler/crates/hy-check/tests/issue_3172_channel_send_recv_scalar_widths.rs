//! issue_3172_channel_send_recv_scalar_widths.rs
//! Tests that channel send statements and receive expressions enforce
//! scalar width and array length compatibility (HYB-E1002) under BUILDABLE.

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
fn chan_send_mismatched_scalar_width_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan int32
    var x int64 = 42
    ch <- x
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_select_send_mismatched_scalar_width_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan int32
    var x int64 = 42
    select {
        ch <- x:
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_mismatched_array_length_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan [2]int
    ch <- [3]int{1, 2, 3}
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_recv_var_init_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan int32
    var y int64 = <-ch
    _ = y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_recv_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan int32
    var y int64
    y = <-ch
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_recv_assign_array_length_mismatch_rejects_e1002() {
    let src = r#"
fn main() {
    var ch chan [2]int
    var y [3]int
    y = <-ch
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_untyped_int_constant_accepts() {
    let src = r#"
fn main() {
    var ch chan int32
    ch <- 42
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn chan_send_matching_scalar_accepts() {
    let src = r#"
fn main() {
    var ch chan int64
    var x int64 = 42
    ch <- x
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
