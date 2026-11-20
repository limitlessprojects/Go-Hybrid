//! Regression tests for foreign curated package function calls (`filepath.glob`, `sort.find`, `io.nop_closer`).
//! Closes open call-arity validation gaps during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Settlement address: 0x759797593fed8f313d8fdba7a3a0d491f0e4b9f7 (USDC on Base L2)

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
fn filepath_glob_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { filepath.glob() }",
        r#"fn main() { filepath.glob("pattern", "extra") }"#,
        r#"fn main() { filepath.glob("pattern", "extra1", "extra2") }"#,
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
fn filepath_glob_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [\"*.txt\"]\n    filepath.glob(xs...)\n}",
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
fn filepath_glob_valid_call_accepted() {
    let src = r#"
fn test_glob() {
    matches := filepath.glob("*.txt")
    _ = matches
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn sort_find_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { sort.find() }",
        "fn main() { sort.find(10) }",
        "fn main() { sort.find(10, fn(i int) int { return 0 }, 42) }",
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
fn sort_find_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [1, 2]\n    sort.find(xs...)\n}",
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
fn sort_find_valid_call_accepted() {
    let src = r#"
fn test_find() {
    sort.find(10, fn(i int) int { return 0 })
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn io_nop_closer_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { io.nop_closer() }",
        r#"fn main() { io.nop_closer("a", "b") }"#,
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
fn local_package_shadowing_preserves_semantics() {
    let src = r#"
fn test_shadowing() {
    var filepath = 42
    _ = filepath
    var sort = "hello"
    _ = sort
    var io = true
    _ = io
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
