//! Regression tests for issue #3330:
//! [soundness] hy-check: untyped slice literals with non-literal heads bypass type checking and fail emission (Row 13 BUILDABLE)

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn test_untyped_slice_variable_head_rejected() {
    let src = r#"package main

fn main() {
    x := 10
    xs := [x]
    println(len(xs))
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject variable head in untyped slice literal");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("untyped slice literal of `x` — spell the element type")),
        "expected HYB-E1002 with spell the element type, got: {:?}",
        errs
    );
}

#[test]
fn test_untyped_slice_call_head_rejected() {
    let src = r#"package main

fn f() int {
    return 42
}

fn main() {
    xs := [f()]
    println(len(xs))
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject call head in untyped slice literal");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("untyped slice literal of `f(...)` — spell the element type")),
        "expected HYB-E1002 with spell the element type, got: {:?}",
        errs
    );
}

#[test]
fn test_untyped_slice_binary_expr_head_rejected() {
    let src = r#"package main

fn main() {
    x := 1
    y := 2
    xs := [x + y]
    println(len(xs))
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject binary expr head in untyped slice literal");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("untyped slice literal of `x + y` — spell the element type")),
        "expected HYB-E1002 with spell the element type, got: {:?}",
        errs
    );
}

#[test]
fn test_untyped_slice_heterogeneous_items_with_var_head_rejected() {
    let src = r#"package main

fn main() {
    x := 10
    xs := [x, "mismatch", true]
    println(len(xs))
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject heterogeneous untyped slice with var head");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_valid_untyped_slice_literals_still_accepted() {
    let src = r#"package main

fn main() {
    xs1 := [1, 2, 3]
    xs2 := ["a", "b"]
    xs3 := [true, false]
    xs4 := [-1, 2]
    xs5 := [(1.5), 2.5]
    println(len(xs1), len(xs2), len(xs3), len(xs4), len(xs5))
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Accept, "valid untyped slice literals must be accepted, got: {:?}", errs);
}
