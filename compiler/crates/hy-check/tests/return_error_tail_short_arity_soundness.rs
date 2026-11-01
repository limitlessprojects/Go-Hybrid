//! return_error_tail_short_arity_soundness.rs — regression tests for Issue #2749:
//! verification that bare return in unnamed error-tail functions, as well as under-arity
//! returns missing >1 values, are rejected with HYB-E1002 under Row 13 (BUILDABLE),
//! adhering to Go spec §Return statements.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn bare_return_unnamed_error_rejects_e1002() {
    let src = "fn f() error {\n    return\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn bare_return_unnamed_multi_result_error_tail_rejects_e1002() {
    let src = "fn g() (int, error) {\n    return\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn under_arity_return_missing_multiple_values_rejects_e1002() {
    let src = "fn h() (int, string, error) {\n    return 1\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn named_result_bare_return_accepts() {
    let cases = [
        "fn f() (err error) {\n    return\n}\n",
        "fn g() (n int, err error) {\n    return\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for named bare return, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn single_omitted_error_tail_sugar_accepts() {
    let cases = [
        "fn f() (int, error) {\n    return 1\n}\n",
        "fn g() (int, string, error) {\n    return 1, \"ok\"\n}\n",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for single omitted error tail, got diagnostics: {:?}",
            r.diagnostics
        );
    }
}
