// Regression tests for #1119: a method whose name collides with a field of the
// same struct receiver must be rejected. Go refuses it with "field and method
// with the same name", so an accepted Hybrid program emitted unbuildable Go.
//
// Negative first, then a positive that pins the near-miss: a method on a
// DIFFERENT struct may reuse a field name that exists elsewhere.

#[test]
fn method_named_like_its_own_struct_field_is_rejected_e1002() {
    let src = "struct counter { n int }\nfn (c counter) n() int { return c.n }\nfn main() {}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 (field and method with the same name), got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn method_with_a_different_name_still_passes() {
    // Guard against an over-strict fix.
    let src = "struct counter { n int }\nfn (c counter) total() int { return c.n }\nfn main() {}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(
        r.verdict,
        hy_check::Verdict::Accept,
        "a method with an unrelated name must still pass, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn method_reusing_a_field_name_from_another_struct_still_passes() {
    // The collision is per receiver type, not global: `n` is a field of
    // `counter`, so it must remain a legal method name on `gauge`.
    let src = "struct counter { n int }\nstruct gauge { v int }\nfn (g gauge) n() int { return g.v }\nfn main() {}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(
        r.verdict,
        hy_check::Verdict::Accept,
        "cross-struct name reuse must stay legal, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| (d.code.clone(), d.message.clone()))
            .collect::<Vec<_>>()
    );
}
