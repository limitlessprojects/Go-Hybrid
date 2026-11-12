//! #3228: slot loan captured by escaping closure must be rejected.
use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test_3228.hy", src).expect("parses");
    check_file(&ast)
}

#[test]
fn slice_slot_loan_captured_by_returned_closure_rejects() {
    let r = check(r#"struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    bump := mk(xs)
    bump()
}
fn mk(xs []cell) fn() {
    p := &xs[0]
    return fn() {
        p.n = 7
    }
}
"#);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r.diagnostics);
}

#[test]
fn named_closure_capturing_slot_loan_rejects() {
    let r = check(r#"struct cell { n int }
fn mk(xs []cell) fn() {
    p := &xs[0]
    f := fn() {
        p.n = 7
    }
    return f
}
"#);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r.diagnostics);
}

#[test]
fn slot_loan_used_inside_fn_not_returned_accepts() {
    let r = check(r#"struct cell { n int }
fn main() {
    xs := []cell{cell{n: 1}}
    p := &xs[0]
    p.n = 7
}
"#);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
