//! bare_return_error_tail_2749.rs — issue #2749 (Row 13 BUILDABLE).
//!
//! `check_return` allowed a short value list whenever the signature ended in
//! `error`: the old `short_ok` was
//! `nv < nw && ((nv == 0 && all_named) || error_tail)` — the `error_tail`
//! exemption applied unconditionally. So a bare `return` in an UNNAMED
//! error-tail signature, and a return missing more than the error, passed
//! `hyb check` and failed `go build` with `not enough return values`.

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
fn bare_return_in_unnamed_error_signature_rejects_e1002() {
    for (name, src) in [
        (
            "fn f() error { return }",
            "fn f() error {\n    return\n}\nfn main() {}\n",
        ),
        (
            "fn g() (int, error) { return }",
            "fn g() (int, error) {\n    return\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn under_arity_return_missing_more_than_error_rejects_e1002() {
    let src = "\
fn h() (int, string, error) {
    return 1
}
fn main() {}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn legal_short_returns_stay_accepted() {
    for (name, src) in [
        (
            "return v -> (int, error) [E3 zero-fill]",
            "fn ok() (int, error) {\n    return 1\n}\nfn main() {}\n",
        ),
        (
            "full arity",
            "fn full() (int, error) {\n    return 1, nil\n}\nfn main() {}\n",
        ),
        (
            "naked return with named results",
            "fn named() (n int, err error) {\n    return\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
