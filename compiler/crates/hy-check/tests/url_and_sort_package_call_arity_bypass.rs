//! Regression tests for Wave 27:
//! Curated foreign package function calls (`url.*`, `sort.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity and non-callable checks on packages `url` and `sort`, invocations on struct/interface types
//! (`url.url()`, `url.values()`, `sort.interface()`), or invocations with invalid arity
//! (`url.parse()`, `url.user_password()`, `sort.ints()`),
//! passed `hyb check` with 0 diagnostics and subsequently failed downstream `go build`.

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

#[test]
fn url_non_callable_types_rejected_e1002() {
    let cases = [
        "fn main() { url.url() }",
        "fn main() { url.values() }",
        "fn main() { url.userinfo() }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn url_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { url.parse() }",
        r#"fn main() { url.parse("a", "b") }"#,
        "fn main() { url.query_escape() }",
        r#"fn main() { url.user_password("alice") }"#,
        "fn main() { url.join_path() }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn url_valid_calls_accepted() {
    let src = r#"
fn caller() error {
    u := url.parse("https://example.com/path")?
    q := url.query_escape("test param")
    up := url.user_password("user", "pass")
    j := url.join_path("https://example.com", "sub", "dir")
    return nil
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn sort_non_callable_types_rejected_e1002() {
    let cases = [
        "fn main() { sort.interface() }",
        "fn main() { sort.int_slice() }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn sort_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { sort.ints() }",
        "fn main() { sort.strings() }",
        "fn main() { sort.search(42) }",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for: {src}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn sort_valid_calls_accepted() {
    let src = r#"
fn test_sort() {
    var nums = [3, 1, 4]
    sort.ints(nums)
    var strs = ["c", "a", "b"]
    sort.strings(strs)
    b := sort.ints_are_sorted(nums)
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
