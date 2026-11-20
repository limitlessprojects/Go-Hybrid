//! issue_3369_chan_conversion_soundness.rs
//!
//! Regression test for issue #3369:
//! Channel type conversions must reject mismatched element scalar widths, signedness,
//! and fixed-size array lengths under HYB-E1002.

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
fn test_reject_chan_scalar_width_mismatch() {
    let src = r#"
package main

type ch32 = chan int32
type ch64 = chan int64

fn main() {
    var c64 ch64
    _ = ch32(c64)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_signedness_mismatch() {
    let src = r#"
package main

type ch_u = chan uint64
type ch_i = chan int64

fn main() {
    var ci ch_i
    _ = ch_u(ci)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_array_len_mismatch() {
    let src = r#"
package main

type ch_arr3 = chan [3]int
type ch_arr4 = chan [4]int

fn main() {
    var c4 ch_arr4
    _ = ch_arr3(c4)
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

type ch_f32 = chan float32
type ch_f64 = chan float64

fn main() {
    var c64 ch_f64
    _ = ch_f32(c64)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_directional_chan_element_mismatch() {
    let src = r#"
package main

type ch_r32 = <-chan int32
type ch_b64 = chan int64

fn main() {
    var c64 ch_b64
    _ = ch_r32(c64)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_fn_return_chan_element_mismatch() {
    let src = r#"
package main

type ch32 = chan int32
type ch64 = chan int64

fn get_chan() ch64 {
    var c ch64
    return c
}

fn main() {
    _ = ch32(get_chan())
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_accept_identical_chan_conversion() {
    let src = r#"
package main

type ch32 = chan int32

fn main() {
    var c ch32
    _ = ch32(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_bidirectional_to_directional_matching_element() {
    let src = r#"
package main

type r32 = <-chan int32
type b32 = chan int32

fn main() {
    var c b32
    _ = r32(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
