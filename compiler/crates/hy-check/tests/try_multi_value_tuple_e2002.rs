//! try_multi_value_tuple_e2002.rs — regression tests for T23 (HYB-E2002)
//! checking of 3+ result calls unwrapped by `?`.
//!
//! Functions returning 3+ results (e.g. `(T1, T2, error)`) unwrapped by `?`
//! produce multiple values. Binding a single variable `p := pair_err()?`
//! is rejected at check time with HYB-E2002 (a multi-value result is not a tuple),
//! catching the mismatch prior to the emitter's MVP-0 3-result cap.
//! Standard 2-result `?`-unwraps (`n := strconv.parse_int(...)?` and pass-through
//! `a, b := single_err()?`) stay accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
fn probe1_try_two_result_single_bind_accepted() {
    let r = check(&with_caller(
        "    n := strconv.parse_int(\"42\", 10, 64)?\n    _ = n\n",
    ));
    assert_eq!(codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn probe2_try_two_result_pass_through_bind_accepted() {
    let r = check(&with_caller(
        "    a, b := single_err()?\n    _ = a\n    _ = b\n",
    ));
    assert_eq!(codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn probe3_try_three_result_single_bind_rejected_e2002() {
    let r = check(&with_caller(
        "    p := pair_err()?\n    _ = p\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn try_three_result_let_and_var_rejected_e2002() {
    let r1 = check(&with_caller(
        "    let p = pair_err()?\n    _ = p\n",
    ));
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);
    assert_eq!(r1.verdict, Verdict::Reject);

    let r2 = check(&with_caller(
        "    var p = pair_err()?\n    _ = p\n",
    ));
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);
    assert_eq!(r2.verdict, Verdict::Reject);
}

#[test]
fn try_three_result_assign_rejected_e2002() {
    let r = check(&with_caller(
        "    var p int\n    p = pair_err()?\n    _ = p\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn try_three_result_exact_bind_accepted_at_check() {
    let r = check(&with_caller(
        "    a, b := pair_err()?\n    _ = a\n    _ = b\n",
    ));
    assert_eq!(codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
