//! issue_3385_equality_type_soundness.rs
//!
//! Regression test for issue #3385:
//! Equality operators (==, !=) must reject pointer, channel, and array alias operands
//! with mismatched scalar widths, float widths, or array lengths under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("parse error");
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
fn test_reject_pointer_scalar_width_mismatch_eq() {
    let src = r#"
package main

fn main() {
    var p1 *int32
    var p2 *int64
    _ = p1 == p2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_scalar_width_mismatch_ne() {
    let src = r#"
package main

fn main() {
    var p1 *int32
    var p2 *int64
    _ = p1 != p2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_float_width_mismatch() {
    let src = r#"
package main

fn main() {
    var p1 *float32
    var p2 *float64
    _ = p1 == p2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_array_length_mismatch() {
    let src = r#"
package main

fn main() {
    var p1 *[3]int
    var p2 *[4]int
    _ = p1 == p2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_scalar_width_mismatch_eq() {
    let src = r#"
package main

fn main() {
    var c1 chan int32
    var c2 chan int64
    _ = c1 == c2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_scalar_width_mismatch_ne() {
    let src = r#"
package main

fn main() {
    var c1 chan int32
    var c2 chan int64
    _ = c1 != c2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_float_width_mismatch() {
    let src = r#"
package main

fn main() {
    var c1 chan float32
    var c2 chan float64
    _ = c1 == c2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_array_length_mismatch() {
    let src = r#"
package main

fn main() {
    var c1 chan [3]int
    var c2 chan [4]int
    _ = c1 == c2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_array_alias_scalar_width_mismatch() {
    let src = r#"
package main

type arr1 = [3]int32
type arr2 = [3]int64

fn main() {
    var a1 arr1
    var a2 arr2
    _ = a1 == a2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_array_alias_length_mismatch() {
    let src = r#"
package main

type arr3 = [3]int
type arr4 = [4]int

fn main() {
    var a1 arr3
    var a2 arr4
    _ = a1 == a2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_accept_pointer_nil_equality() {
    let src = r#"
package main

fn main() {
    var p *int32
    _ = p == nil
    _ = nil == p
    _ = p != nil
    _ = nil != p
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_chan_nil_equality() {
    let src = r#"
package main

fn main() {
    var c chan int32
    _ = c == nil
    _ = nil == c
    _ = c != nil
    _ = nil != c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_identical_pointer_equality() {
    let src = r#"
package main

fn main() {
    var p1 *int32
    var p2 *int32
    _ = p1 == p2
    _ = p1 != p2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_identical_chan_equality() {
    let src = r#"
package main

fn main() {
    var c1 chan int32
    var c2 chan int32
    _ = c1 == c2
    _ = c1 != c2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_identical_array_alias_equality() {
    let src = r#"
package main

type arr1 = [3]int32

fn main() {
    var a1 arr1
    var a2 arr1
    _ = a1 == a2
    _ = a1 != a2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
