//! issue_3280_append_array_dim_and_nested_slice_soundness.rs — regression tests
//! for append fixed-size array dimension and nested slice element scalar soundness
//! under Row 13 (BUILDABLE) and HYB-E1002 (issue #3280).

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
fn append_mismatched_array_dimension_rejected() {
    let src = r#"
package main

fn test_append_arr() {
    var s [][2]int
    var a [3]int
    s = append(s, a)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "appending [3]int to [][2]int must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_matching_array_dimension_accepted() {
    let src = r#"
package main

fn test_append_arr_ok() {
    var s [][2]int
    var a [2]int
    s = append(s, a)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "appending [2]int to [][2]int must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn append_spread_mismatched_array_dimension_rejected() {
    let src = r#"
package main

fn test_append_spread_arr() {
    var s [][2]int
    var b [][3]int
    s = append(s, b...)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "spread appending [][3]int to [][2]int must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_spread_matching_array_dimension_accepted() {
    let src = r#"
package main

fn test_append_spread_arr_ok() {
    var s [][2]int
    var b [][2]int
    s = append(s, b...)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "spread appending [][2]int to [][2]int must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn append_nested_slice_mismatched_scalar_rejected() {
    let src = r#"
package main

fn test_append_nested() {
    var s [][]int64
    var item []int32
    s = append(s, item)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "appending []int32 to [][]int64 must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_nested_slice_matching_scalar_accepted() {
    let src = r#"
package main

fn test_append_nested_ok() {
    var s [][]int64
    var item []int64
    s = append(s, item)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "appending []int64 to [][]int64 must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn append_nested_slice_spread_mismatched_scalar_rejected() {
    let src = r#"
package main

fn test_append_nested_spread() {
    var s [][]int64
    var b [][]int32
    s = append(s, b...)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "spread appending [][]int32 to [][]int64 must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn append_nested_slice_spread_matching_scalar_accepted() {
    let src = r#"
package main

fn test_append_nested_spread_ok() {
    var s [][]int64
    var b [][]int64
    s = append(s, b...)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "spread appending [][]int64 to [][]int64 must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn append_alias_slice_mismatched_array_rejected() {
    let src = r#"
package main

type my_grid = [][2]int

fn test_alias_append() {
    var s my_grid
    var a [3]int
    s = append(s, a)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "appending [3]int to my_grid ([][2]int) must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}
