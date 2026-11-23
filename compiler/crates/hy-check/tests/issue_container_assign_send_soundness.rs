//! issue_container_assign_send_soundness.rs — Row 13 BUILDABLE soundness.
//!
//! Regression tests for HYB-E1002: container element scalar width mismatches
//! ([]int64 → []int32, map[K]int64 → map[K]int32, *int64 → *int32) that
//! bypass single assignment, parallel assignment, and channel send checks.

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
fn single_assign_int64_slice_to_int32_rejected() {
    let r = check(
        r#"fn main() {
    var s32 []int32
    var s64 []int64
    s32 = s64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn single_assign_int64_map_to_int32_rejected() {
    let r = check(
        r#"fn main() {
    var m32 map[string]int32
    var m64 map[string]int64
    m32 = m64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn single_assign_int64_ptr_to_int32_rejected() {
    let r = check(
        r#"fn main() {
    var p32 *int32
    var p64 *int64
    p32 = p64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn parallel_assign_container_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var s1 []int32
    var s2 []int64
    var m1 map[string]int32
    var m2 map[string]int64
    s1, m1 = s2, m2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_int64_slice_to_int32_chan_rejected() {
    let r = check(
        r#"fn main() {
    var ch chan []int32
    var s64 []int64
    ch <- s64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_int64_map_to_int32_chan_rejected() {
    let r = check(
        r#"fn main() {
    var ch chan map[string]int32
    var m64 map[string]int64
    ch <- m64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_send_int64_ptr_to_int32_chan_rejected() {
    let r = check(
        r#"fn main() {
    var ch chan *int32
    var p64 *int64
    ch <- p64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn assign_same_container_scalar_accepted() {
    let r = check(
        r#"fn main() {
    var s1 []int32
    var s2 []int32
    s1 = s2
    var ch chan []int32
    ch <- s2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected accept: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected no diagnostics: {:?}", r.diagnostics);
}
