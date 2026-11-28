//! issue_2859_field_index_scalar_soundness.rs — struct field and slice/array index scalar type checking
//!
//! Issue #2859: struct field and slice/array index assignments allow mismatched scalar types (HYB-E1002).

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
fn struct_field_scalar_mismatch_rejected() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var a32 int32 = 42
    p.x = a32
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "assigning int32 variable to int64 struct field must be rejected"
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn struct_field_float_scalar_mismatch_rejected() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var f32 float32 = 3.14
    p.f = f32
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "assigning float32 variable to float64 struct field must be rejected"
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_element_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var xs []int64 = []int64{1, 2, 3}
    var a32 int32 = 42
    xs[0] = a32
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "assigning int32 variable to []int64 slice element must be rejected"
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn array_element_scalar_mismatch_rejected() {
    let src = r#"
fn main() {
    var arr [3]int64
    var a32 int32 = 42
    arr[0] = a32
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "assigning int32 variable to [3]int64 array element must be rejected"
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn field_and_slice_compound_assign_scalar_mismatch_rejected() {
    let src1 = r#"
struct point {
    x int64
}

fn main() {
    var p point
    var a32 int32 = 42
    p.x += a32
}
"#;
    let r1 = check(src1);
    assert_eq!(
        r1.verdict,
        Verdict::Reject,
        "p.x += a32 between int64 and int32 must be rejected"
    );
    assert!(error_codes(&r1).iter().any(|c| c == "HYB-E1002"));

    let src2 = r#"
fn main() {
    var xs []int64 = []int64{1, 2}
    var a32 int32 = 42
    xs[0] += a32
}
"#;
    let r2 = check(src2);
    assert_eq!(
        r2.verdict,
        Verdict::Reject,
        "xs[0] += a32 between int64 and int32 must be rejected"
    );
    assert!(error_codes(&r2).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn reading_field_and_slice_element_rejects_assignment_to_mismatched_target() {
    let src = r#"
struct point {
    x int64
}

fn main() {
    var p point
    var target int32
    target = p.x
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "target (int32) = p.x (int64) must be rejected"
    );
    assert!(error_codes(&r).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn positive_controls_accept_matching_scalars_and_untyped_constants() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var a64 int64 = 100
    var f64 float64 = 2.718
    p.x = a64
    p.f = f64
    p.x = 42
    p.f = 3.14

    var xs []int64 = []int64{1, 2, 3}
    xs[0] = a64
    xs[0] = 7

    var arr [2]float64
    arr[0] = f64
    arr[1] = 1.0

    p.x += 10
    xs[0] += a64
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "matching scalar assignments and untyped constants must be accepted, got: {:?}",
        r.diagnostics
    );
}
