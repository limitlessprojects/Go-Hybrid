//! multi_value_width_mismatch_rejected.rs — #4576: a parallel assignment from a
//! multi-value call must reject a scalar-width mismatch.
//!
//! Before this fix, `AssignOp::Assign`'s single-target branch checked both
//! `kinds_conflict` and `scalar_mismatch`, while the `targets.len() > 1 &&
//! values.len() == 1` branch checked ONLY `kinds_conflict`. Since `int32` and
//! `int64` both map to `OpKind::Int`, this was accepted at check time and the
//! emitted Go failed to build:
//!
//!   error: cannot use pair() (value of type int32) as int64 value in assignment
//!
//! GUARANTEES.md Row 13 (BUILDABLE) / INVARIANT 4 says an accepted program
//! builds, so this is a soundness hole, not a wording issue.
//!
//! The two accept controls are essential: a blanket rejection of multi-value
//! assignment would pass the reject test and break legal code.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn int64_targets_from_int32_results_are_rejected() {
    let src = r#"
fn pair() (int32, int32) {
    return 3, 4
}

fn main() {
    var a int64 = 10
    var b int64 = 20
    a, b = pair()
    _ = a
    _ = b
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "int64, int64 = pair() (int32, int32) must reject, got {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| {
            d.code.to_string() == "HYB-E1002" && d.message.contains("`int32`")
        }),
        "expected HYB-E1002 naming int32, got {:?}",
        res.diagnostics
    );
}

#[test]
fn float64_targets_from_float32_results_are_rejected() {
    let src = r#"
fn pairf() (float32, float32) {
    return 1.5, 2.5
}

fn main() {
    var a float64 = 1.0
    var b float64 = 2.0
    a, b = pairf()
    _ = a
    _ = b
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "float64, float64 = pairf() (float32, float32) must reject, got {:?}",
        res.diagnostics
    );
}

#[test]
fn control_matching_width_still_accepts() {
    // Kiln for over-reach: identical widths are legal Go and must stay legal.
    let src = r#"
fn pair() (int64, int64) {
    return 3, 4
}

fn main() {
    var a int64 = 10
    var b int64 = 20
    a, b = pair()
    _ = a
    _ = b
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "matching-width multi-value assignment must stay accepted, got {:?}",
        res.diagnostics
    );
}

#[test]
fn control_channel_receive_still_accepts() {
    // The other shape in the same branch: a receive yields (T, bool). Its
    // second result is bool, so a name comparison must not fire where Go is
    // happy — `v, ok := <-ch` with an int channel.
    let src = r#"
fn main() {
    ch := make(chan int64, 1)
    var v int64 = 0
    var ok bool = false
    v, ok = <-ch
    _ = v
    _ = ok
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "v, ok = <-ch must stay accepted, got {:?}",
        res.diagnostics
    );
}
