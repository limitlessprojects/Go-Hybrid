//! issue_3364_pointer_conversion_soundness.rs
//!
//! Regression test for issue #3364:
//! Pointer type conversions must reject mismatched base types (scalar widths,
//! signedness, and fixed-size array lengths) under HYB-E1002.

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
fn test_reject_pointer_int_scalar_width_mismatch() {
    let src = r#"
package main

type p32 = *int32
type p64 = *int64

fn main() {
    var p p64
    _ = p32(p)
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

type parr3 = *[3]int
type parr4 = *[4]int

fn main() {
    var p parr4
    _ = parr3(p)
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

type pf32 = *float32
type pf64 = *float64

fn main() {
    var p pf64
    _ = pf32(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_sign_mismatch() {
    let src = r#"
package main

type pu64 = *uint64
type pi64 = *int64

fn main() {
    var p pi64
    _ = pu64(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_addrof_scalar_mismatch() {
    let src = r#"
package main

type p32 = *int32

fn main() {
    var x int64
    _ = p32(&x)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_pointer_addrof_array_length_mismatch() {
    let src = r#"
package main

type parr3 = *[3]int

fn main() {
    var a [4]int
    _ = parr3(&a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_accept_identical_pointer_conversion() {
    let src = r#"
package main

type p32 = *int32

fn main() {
    var p p32
    _ = p32(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

