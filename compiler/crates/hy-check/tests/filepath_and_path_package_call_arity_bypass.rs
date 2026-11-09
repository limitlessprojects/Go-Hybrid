//! Regression tests for foreign package function calls (`filepath.*`, `path.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).

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
fn filepath_non_callable_members_rejected_e1002() {
    let cases = [
        "fn main() { filepath.separator() }",
        "fn main() { filepath.list_separator() }",
        "fn main() { filepath.skip_dir() }",
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
fn filepath_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { filepath.clean() }",
        r#"fn main() { filepath.dir("a", "b") }"#,
        "fn main() { filepath.base() }",
        r#"fn main() { filepath.rel("only_one") }"#,
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
fn filepath_valid_calls_accepted() {
    let src = r#"
fn test_filepath() {
    c := filepath.clean("/foo/bar/../baz")
    _ = c
    d := filepath.dir("/foo/bar")
    _ = d
    b := filepath.base("/foo/bar.txt")
    _ = b
    e := filepath.ext("/foo/bar.txt")
    _ = e
    j := filepath.join("a", "b", "c")
    _ = j
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn path_non_callable_members_rejected_e1002() {
    let r = check("fn main() { path.err_bad_pattern() }");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn path_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { path.clean() }",
        r#"fn main() { path.dir("a", "b") }"#,
        r#"fn main() { path.base("a", "b") }"#,
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
fn path_valid_calls_accepted() {
    let src = r#"
fn test_path() {
    c := path.clean("a/b/c")
    _ = c
    d := path.dir("a/b")
    _ = d
    b := path.base("a/b")
    _ = b
    j := path.join("a", "b")
    _ = j
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
