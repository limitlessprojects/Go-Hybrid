//! issue_3283_shift_unsigned_count_soundness.rs — regression tests for
//! shift operator and compound shift assignment unsigned shift count soundness
//! under Row 13 (BUILDABLE) and HYB-E1002 (issue #3283).

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
fn shift_left_signed_int_rejected() {
    let src = r#"
package main

fn test_shl_signed() {
    var x int = 1
    var s int = 2
    _ = x << s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "shift left with signed int count must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn shift_right_signed_int64_rejected() {
    let src = r#"
package main

fn test_shr_signed() {
    var x int = 16
    var s int64 = 2
    _ = x >> s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "shift right with signed int64 count must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn compound_shl_signed_int32_rejected() {
    let src = r#"
package main

fn test_shl_assign_signed() {
    var x int = 1
    var s int32 = 3
    x <<= s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "compound shift left with signed int32 count must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn compound_shr_signed_int16_rejected() {
    let src = r#"
package main

fn test_shr_assign_signed() {
    var x int = 64
    var s int16 = 2
    x >>= s
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "compound shift right with signed int16 count must be rejected"
    );
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn shift_left_unsigned_uint_accepted() {
    let src = r#"
package main

fn test_shl_unsigned() {
    var x int = 1
    var u uint = 2
    _ = x << u
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "shift left with unsigned uint count must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn shift_right_unsigned_uint32_accepted() {
    let src = r#"
package main

fn test_shr_unsigned() {
    var x int = 16
    var u uint32 = 2
    _ = x >> u
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "shift right with unsigned uint32 count must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn compound_shl_unsigned_uint8_accepted() {
    let src = r#"
package main

fn test_shl_assign_unsigned() {
    var x int = 1
    var u uint8 = 3
    x <<= u
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "compound shift left with unsigned uint8 count must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn shift_untyped_constant_accepted() {
    let src = r#"
package main

fn test_shl_const() {
    var x int = 1
    _ = x << 2
    _ = x >> 1
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "shift with untyped constant count must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}

#[test]
fn compound_shift_untyped_constant_accepted() {
    let src = r#"
package main

fn test_compound_const() {
    var x int = 1
    x <<= 3
    x >>= 1
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "compound shift with untyped constant count must be accepted"
    );
    assert!(error_codes(&r).is_empty());
}
