//! e1002_names_the_declared_scalar_type.rs — HYB-E1002 must name the DECLARED
//! scalar type of each side, not the OpKind class.
//!
//! Before this test, a `int64 = int32` assignment reported
//!   cannot assign a value of type `int` to `a` (type `int`)
//! — both sides labelled with the class `int`, so the message reads as
//! int-to-int and hides why Go refuses. The precise declared names were
//! already computed for the sized-array check in the same arm; they just were
//! not used in the message.
//!
//! The control case pins the other direction: a genuine `int` source must KEEP
//! reporting `int`. A fix that simply printed the wider name everywhere would
//! satisfy the first case and break the second.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_msg(r: &hy_check::CheckResult, needle: &str) -> bool {
    r.diagnostics.iter().any(|d| d.message.contains(needle))
}

#[test]
fn int64_from_int32_names_both_declared_types() {
    let src = r#"
fn test_width_mismatch() {
    var a int64 = 10
    var b int32 = 3
    a = b
    _ = a
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "int64 = int32 must reject");
    assert!(
        has_msg(&res, "cannot assign a value of type `int32` to `a` (type `int64`)"),
        "message must name int32 -> int64, got {:?}",
        res.diagnostics
    );
}

#[test]
fn float64_from_float32_names_both_declared_types() {
    let src = r#"
fn test_float_width_mismatch() {
    var a float64 = 1.5
    var b float32 = 2.5
    a = b
    _ = a
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "float64 = float32 must reject");
    assert!(
        has_msg(&res, "cannot assign a value of type `float32` to `a` (type `float64`)"),
        "message must name float32 -> float64, got {:?}",
        res.diagnostics
    );
}

#[test]
fn control_plain_int_source_still_reports_int() {
    // The other direction: `var i int` really is `int`, so the message must NOT
    // be widened to some other name. This is the counter-probe for a fix that
    // over-reaches.
    let src = r#"
fn test_int_to_float() {
    var f float64 = 1.0
    var i int = 1
    f = i
    _ = f
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "float64 = int must reject");
    assert!(
        has_msg(&res, "cannot assign a value of type `int` to `f` (type `float64`)"),
        "message must keep naming `int` for a plain int source, got {:?}",
        res.diagnostics
    );
}
