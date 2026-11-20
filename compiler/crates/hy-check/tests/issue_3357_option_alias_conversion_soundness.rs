//! issue_3357_option_alias_conversion_soundness.rs
//!
//! Regression test for issue #3357:
//! Type alias and direct conversions to `option[T]` must reject non-option operands and
//! options with mismatched payload kinds, scalar widths, or array lengths under HYB-E1002.

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
fn test_reject_scalar_to_option_alias() {
    let src = r#"
package main

type opt_i = option[int]

fn main() {
    _ = opt_i(42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_scalar_to_direct_option() {
    let src = r#"
package main

fn main() {
    _ = option[int](42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_string_to_option_alias() {
    let src = r#"
package main

type opt_i = option[int]

fn main() {
    _ = opt_i("hello")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_slice_to_option_alias() {
    let src = r#"
package main

type opt_slice = option[[]int]

fn main() {
    var xs []int
    _ = opt_slice(xs)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_map_to_option_alias() {
    let src = r#"
package main

type opt_map = option[map[string]int]

fn main() {
    var m map[string]int
    _ = opt_map(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_option_payload_kind_mismatch() {
    let src = r#"
package main

type opt_i = option[int]
type opt_s = option[string]

fn main() {
    var s opt_s
    _ = opt_i(s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_option_scalar_width_mismatch() {
    let src = r#"
package main

type opt_i32 = option[int32]
type opt_i64 = option[int64]

fn main() {
    var o64 opt_i64
    _ = opt_i32(o64)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_option_array_length_mismatch() {
    let src = r#"
package main

type opt_arr3 = option[[3]int]
type opt_arr4 = option[[4]int]

fn main() {
    var o4 opt_arr4
    _ = opt_arr3(o4)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_reject_option_to_scalar() {
    let src = r#"
package main

type opt_i = option[int]

fn main() {
    var o opt_i
    _ = int(o)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn test_accept_identical_option_conversion() {
    let src = r#"
package main

type opt_i = option[int]

fn main() {
    var o opt_i
    _ = opt_i(o)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty());
}
