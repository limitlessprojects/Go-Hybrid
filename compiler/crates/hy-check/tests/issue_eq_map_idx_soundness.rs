//! issue_eq_map_idx_soundness.rs — Row 13 BUILDABLE soundness.
//!
//! Regression tests for HYB-E1002: inner scalar and sized array mismatches
//! that bypass equality comparisons (==, != on pointers, channels, arrays)
//! and map index expressions (map[[N]T], map[*T]).

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
fn eq_ptr_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var p32 *int32
    var p64 *int64
    _ = p32 == p64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn ne_ptr_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var p32 *int32
    var p64 *int64
    _ = p32 != p64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn eq_chan_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var ch32 chan int32
    var ch64 chan int64
    _ = ch32 == ch64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn ne_chan_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var ch32 chan int32
    var ch64 chan int64
    _ = ch32 != ch64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn eq_sized_array_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var a32 [4]int32
    var a64 [4]int64
    _ = a32 == a64
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_index_sized_array_len_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var m map[[4]int]string
    var k [3]int
    _ = m[k]
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_index_ptr_scalar_mismatch_rejected() {
    let r = check(
        r#"fn main() {
    var m map[*int32]string
    var p *int64
    _ = m[p]
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn eq_and_map_index_valid_matching_types_accepted() {
    let r = check(
        r#"fn main() {
    var p1 *int32
    var p2 *int32
    _ = p1 == p2

    var ch1 chan int32
    var ch2 chan int32
    _ = ch1 == ch2

    var a1 [4]int32
    var a2 [4]int32
    _ = a1 == a2

    var m map[[4]int]string
    var k [4]int
    _ = m[k]
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected accept: {:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "expected no diagnostics: {:?}", r.diagnostics);
}
