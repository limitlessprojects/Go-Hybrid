//! signature_identifier_uniqueness.rs — regression for issue #1409.
//!
//! Receivers, value parameters, and named results share one Go function
//! scope. Repeating a non-blank identifier makes the emitted Go unbuildable,
//! so `hyb check` must reject it as HYB-E1002 (rule BUILDABLE).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {codes:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn accepts(src: &str, what: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "{what} must stay accepted, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

#[test]
fn duplicate_value_parameters_reject() {
    rejects_with_e1002(
        r#"fn add(a int, a int) int {
    return a
}
fn main() {
    fmt.println("{add(1, 2)}")
}
"#,
        "duplicate value parameters",
    );
}

#[test]
fn duplicate_named_results_reject() {
    rejects_with_e1002(
        r#"fn two() (a int, a int) {
    a = 1
    return
}
fn main() {
    x, y := two()
    fmt.println("{x} {y}")
}
"#,
        "duplicate named results",
    );
}

#[test]
fn parameter_and_named_result_collision_rejects() {
    rejects_with_e1002(
        r#"fn value(a int) (a int) {
    a = 1
    return
}
fn main() {
    fmt.println("{value(0)}")
}
"#,
        "parameter/result collision",
    );
}

#[test]
fn receiver_and_parameter_collision_rejects() {
    rejects_with_e1002(
        r#"struct counter {
    n int
}
fn (c counter) value(c int) int {
    return c
}
fn main() {
    c := counter{n: 0}
    fmt.println("{c.value(1)}")
}
"#,
        "receiver/parameter collision",
    );
}

#[test]
fn duplicate_closure_parameters_reject() {
    rejects_with_e1002(
        r#"fn main() {
    f := fn(a int, a int) int {
        return a
    }
    fmt.println("{f(1, 2)}")
}
"#,
        "duplicate closure parameters",
    );
}

#[test]
fn duplicate_closure_named_results_reject() {
    rejects_with_e1002(
        r#"fn main() {
    f := fn() (a int, a int) {
        a = 1
        return
    }
    x, y := f()
    fmt.println("{x} {y}")
}
"#,
        "duplicate closure named results",
    );
}

#[test]
fn distinct_and_unnamed_signatures_stay_accepted() {
    accepts(
        r#"fn named(a int) (x int, y int) {
    x = a
    y = 2
    return
}
fn unnamed() (int, int) {
    return 3, 4
}
fn main() {
    x, y := named(1)
    z, w := unnamed()
    fmt.println("{x} {y} {z} {w}")
}
"#,
        "distinct named results and unnamed results",
    );
}

#[test]
fn repeated_blank_parameters_and_results_stay_accepted() {
    accepts(
        r#"fn blanks(_ int, _ int) (_ int, _ int) {
    return 1, 2
}
fn main() {
    f := fn(_ int, _ int) (_ int, _ int) {
        return 3, 4
    }
    x, y := blanks(1, 2)
    z, w := f(3, 4)
    fmt.println("{x} {y} {z} {w}")
}
"#,
        "repeated blank identifiers",
    );
}

#[test]
fn nested_function_scopes_may_reuse_names() {
    accepts(
        r#"fn outer(a int) int {
    f := fn(a int) int {
        return a
    }
    return f(a)
}
fn main() {
    fmt.println("{outer(1)}")
}
"#,
        "independent nested function scopes",
    );
}
