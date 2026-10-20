//! multivalue_call_args_1015.rs — regression tests for issue #1015.
//!
//! A lone multi-value call used as the whole argument list expands to one
//! argument per result (`g(two())`). Before the fix three paths disagreed:
//! the argument *type* checks returned early on multi-value (accepting
//! `takes_ints(returns_strings())`, whose Go does not build), while the method
//! and local-closure *arity* checks counted it as one argument (falsely
//! rejecting the valid `s.takes_two(two())`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_accept(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected ACCEPT ({}), got {:?} with {:?}",
        what,
        r.verdict,
        codes(&r)
    );
}

fn assert_e1002(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 ({}), got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

// --- soundness: multi-value results must match the parameter types ---------

#[test]
fn multivalue_argument_types_are_checked() {
    assert_e1002(
        r#"
fn takes_ints(a int, b int) {}
fn returns_strings() (string, string) { return "a", "b" }
fn main() {
    takes_ints(returns_strings())
}
"#,
        "string results into int parameters",
    );
}

#[test]
fn multivalue_argument_types_are_checked_at_task_spawn() {
    assert_e1002(
        r#"
fn takes_ints(a int, b int) {}
fn returns_strings() (string, string) { return "a", "b" }
fn main() {
    task takes_ints(returns_strings())
    for i in 0..1 {}
}
"#,
        "same bypass at a task spawn site",
    );
}

#[test]
fn multivalue_argument_types_are_checked_on_methods() {
    assert_e1002(
        r#"
struct st {}
fn (s st) takes_ints(a int, b int) {}
fn returns_strings() (string, string) { return "a", "b" }
fn main() {
    var s st
    s.takes_ints(returns_strings())
}
"#,
        "string results into int method parameters",
    );
}

// --- precision: valid multi-value calls must not be rejected ---------------

#[test]
fn multivalue_into_matching_parameters_is_accepted() {
    assert_accept(
        r#"
fn takes_strs(a string, b string) {}
fn returns_strings() (string, string) { return "a", "b" }
fn main() {
    takes_strs(returns_strings())
}
"#,
        "two string results into two string parameters",
    );
}

#[test]
fn multivalue_into_matching_method_parameters_is_accepted() {
    assert_accept(
        r#"
struct st {}
fn (s st) takes_two(a int, b int) {}
fn two() (int, int) { return 1, 2 }
fn main() {
    var s st
    s.takes_two(two())
}
"#,
        "method called with a lone multi-value call",
    );
}

#[test]
fn multivalue_into_matching_closure_parameters_is_accepted() {
    assert_accept(
        r#"
fn two() (int, int) { return 1, 2 }
fn main() {
    f := fn(a int, b int) {}
    f(two())
}
"#,
        "closure called with a lone multi-value call",
    );
}

#[test]
fn multivalue_into_too_few_closure_parameters_rejects() {
    assert_e1002(
        r#"
fn two() (int, int) { return 1, 2 }
fn main() {
    f := fn(a int) {}
    f(two())
}
"#,
        "two results into a one-parameter closure",
    );
}
