//! issue_3136_indexed_field_scalar_widths.rs — issue #3136 (Row 13 BUILDABLE).
//!
//! Reading slice/array elements `s[0]`, `a[0]` and struct fields `p.x` evaluates
//! to concrete numeric scalar widths (e.g. `int8`). When used in scalar-checked
//! positions expecting a different numeric width (`int64`), `typed_value_scalar`
//! previously returned `None` (falling through to unknown), causing mismatched
//! assignments, call arguments, slice literals, and append operations to pass
//! `hyb check` and fail `go build` with `cannot use ... as ... value`.

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
        .map(|d| d.code.as_str().to_string())
        .collect()
}

#[test]
fn slice_index_assigned_to_mismatched_scalar_rejected() {
    let src = r#"
fn main() {
    var s []int8
    var b int64 = s[0]
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn array_index_assigned_to_mismatched_scalar_rejected() {
    let src = r#"
fn main() {
    var a [4]int8
    var b int64 = a[0]
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn struct_field_assigned_to_mismatched_scalar_rejected() {
    let src = r#"
struct point {
    x int8
}

fn main() {
    var p point
    var b int64 = p.x
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for struct field scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn function_call_argument_indexed_mismatch_rejected() {
    let src = r#"
fn take(x int64) {}

fn main() {
    var s []int8
    take(s[0])
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for call argument scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_literal_with_mismatched_field_rejected() {
    let src = r#"
struct point {
    x int8
}

fn main() {
    var p point
    s := []int64{p.x}
    _ = s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice literal field scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn append_with_mismatched_indexed_element_rejected() {
    let src = r#"
fn main() {
    var s []int8
    var dst []int64
    dst = append(dst, s[0])
    _ = dst
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for append scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn matching_scalar_widths_stay_accepted() {
    let src = r#"
struct point {
    x int64
}

fn take(x int64) {}

fn main() {
    var s []int64
    var b int64 = s[0]
    var p point
    var c int64 = p.x
    take(s[0])
    take(p.x)
    _ = b
    _ = c
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "matching scalar widths must be accepted, got {:?}",
        r.diagnostics
    );
}
