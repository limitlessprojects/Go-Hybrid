//! try_multi_value_tuple_e2002.rs — regression tests for T23 (HYB-E2002)
//! interaction with error unwrapping `?` (E3/E4).
//!
//! SPEC-LANGUAGE.md §2.8 pins that a multi-value result is not a tuple.
//! When a function returns `(T1, T2, error)`, applying `?` yields two values.
//! Binding one name `p := f()?` must be rejected with HYB-E2002, preventing
//! `go build` from failing with `assignment mismatch: 2 variables but f returns 3 values`.
//! Symmetrically, binding two names to a `(T, error)` call unwrapped by `?`
//! must be rejected with HYB-E1002 (assignment mismatch).

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

const DECLS: &str = r#"
fn pair_err() (int, string, error) {
    return 1, "ok", nil
}

fn single_err() (int, error) {
    return 42, nil
}
"#;

fn with_caller(body: &str) -> String {
    format!("{DECLS}\nfn caller() error {{\n{body}\n    return nil\n}}\n")
}

#[test]
fn try_multi_value_single_bind_rejected_e2002() {
    let r = check(&with_caller("    p := pair_err()?"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn try_multi_value_exact_bind_accepted() {
    let r = check(&with_caller("    a, b := pair_err()?"));
    assert_eq!(codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn try_single_value_over_bind_rejected_e1002() {
    let r = check(&with_caller("    a, b := single_err()?"));
    assert_eq!(codes(&r), vec!["HYB-E1002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn try_multi_value_let_and_var_rejected_e2002() {
    let r1 = check(&with_caller("    let p = pair_err()?"));
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);

    let r2 = check(&with_caller("    var p = pair_err()?"));
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);
}

#[test]
fn try_multi_value_assign_rejected_e2002() {
    let r = check(&with_caller("    var p int\n    p = pair_err()?"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn try_foreign_package_over_bind_rejected_e1002() {
    let r = check(&with_caller(r#"    a, b := strconv.parse_int("42", 10, 64)?"#));
    assert_eq!(codes(&r), vec!["HYB-E1002".to_string()]);
}

#[test]
fn try_foreign_package_exact_bind_accepted() {
    let r = check(&with_caller(r#"    n := strconv.parse_int("42", 10, 64)?"#));
    assert_eq!(codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
