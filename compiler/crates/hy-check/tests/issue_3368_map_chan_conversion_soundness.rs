//! issue_3368_map_chan_conversion_soundness.rs
//!
//! Regression test for issue #3368:
//! Map and channel type conversions must reject mismatched key, value, and element
//! scalar widths, signs, and array lengths under HYB-E1002.

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
fn test_reject_map_value_scalar_width_mismatch() {
    let src = r#"
package main

type m32 = map[string]int32
type m64 = map[string]int64

fn main() {
    var m m64
    _ = m32(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_key_scalar_width_mismatch() {
    let src = r#"
package main

type mk32 = map[int32]string
type mk64 = map[int64]string

fn main() {
    var m mk64
    _ = mk32(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_key_array_length_mismatch() {
    let src = r#"
package main

type mkarr3 = map[[3]int]string
type mkarr4 = map[[4]int]string

fn main() {
    var m mkarr4
    _ = mkarr3(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_value_array_length_mismatch() {
    let src = r#"
package main

type marr3 = map[string][3]int
type marr4 = map[string][4]int

fn main() {
    var m marr4
    _ = marr3(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_value_float_width_mismatch() {
    let src = r#"
package main

type mf32 = map[string]float32
type mf64 = map[string]float64

fn main() {
    var m mf64
    _ = mf32(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_make_call_mismatch() {
    let src = r#"
package main

type m32 = map[string]int32
type m64 = map[string]int64

fn main() {
    _ = m32(make(m64))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_element_scalar_width_mismatch() {
    let src = r#"
package main

type c32 = chan int32
type c64 = chan int64

fn main() {
    var c c64
    _ = c32(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_element_float_width_mismatch() {
    let src = r#"
package main

type cf32 = chan float32
type cf64 = chan float64

fn main() {
    var c cf64
    _ = cf32(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_element_array_length_mismatch() {
    let src = r#"
package main

type carr3 = chan [3]int
type carr4 = chan [4]int

fn main() {
    var c carr4
    _ = carr3(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_chan_make_call_mismatch() {
    let src = r#"
package main

type c32 = chan int32

fn main() {
    _ = c32(make(chan int64, 1))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_accept_identical_map_conversion() {
    let src = r#"
package main

type m32 = map[string]int32

fn main() {
    var m m32
    _ = m32(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn test_accept_identical_chan_conversion() {
    let src = r#"
package main

type c32 = chan int32

fn main() {
    var c c32
    _ = c32(c)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
