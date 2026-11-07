// Regression tests for #1673: `append` with fewer than two arguments must be
// rejected. The builtin needs a container and at least one element; the one- and
// zero-argument forms used to be accepted and silently emitted a call that cannot
// mean what the source says.
//
// The three tests are deliberately a set: two negative (the forms that must now
// fail) and one positive (the form that must keep working). A fix that rejects
// everything would pass the first two and fail the third.

#[test]
fn append_with_one_argument_is_rejected_e2005() {
    let src = "fn main() {\n    xs := []int{}\n    ys := append(xs)\n    _ = ys\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn append_with_no_arguments_is_rejected_e2005() {
    let src = "fn main() {\n    ys := append()\n    _ = ys\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| d.code.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn append_with_two_arguments_still_passes() {
    // Guard against an over-strict fix: the valid form must keep passing.
    // `_ = ys` consumes the binding, otherwise HYB-E1002 (declared and never
    // used) would reject this for a reason that has nothing to do with arity.
    let src = "fn main() {\n    xs := []int{}\n    ys := append(xs, 1)\n    _ = ys\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(
        r.verdict,
        hy_check::Verdict::Accept,
        "valid append must still pass, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}
