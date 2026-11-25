//! Regression tests for foreign package function calls (`sync.*`).
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
fn sync_non_callable_types_rejected_e1002() {
    let cases = [
        "fn main() { sync.mutex() }",
        "fn main() { sync.rw_mutex() }",
        "fn main() { sync.wait_group() }",
        "fn main() { sync.once() }",
        "fn main() { sync.cond() }",
        "fn main() { sync.pool() }",
        "fn main() { sync.locker() }",
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
fn sync_arity_mismatch_rejected_e1002() {
    let cases = [
        "fn main() { sync.new_cond() }",
        "fn main() { sync.new_cond(1, 2) }",
        "fn main() { sync.once_func() }",
        "fn main() { sync.once_func(1, 2) }",
        "fn main() { sync.once_value() }",
        "fn main() { sync.once_value(1, 2) }",
        "fn main() { sync.once_values() }",
        "fn main() { sync.once_values(1, 2) }",
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
fn sync_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [1]\n    sync.new_cond(xs...)\n}",
        "fn main() {\n    var xs = [1]\n    sync.once_func(xs...)\n}",
        "fn main() {\n    var xs = [1]\n    sync.once_value(xs...)\n}",
        "fn main() {\n    var xs = [1]\n    sync.once_values(xs...)\n}",
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
fn sync_valid_calls_accepted() {
    let src = r#"
fn test_sync() {
    f := sync.once_func(fn() {})
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}

