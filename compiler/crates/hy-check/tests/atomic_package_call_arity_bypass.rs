//! Regression tests for foreign package function calls (`atomic.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE, fixes #4169).

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
fn atomic_non_callable_types_rejected_e1002() {
    let cases = [
        "fn main() { atomic.bool() }",
        "fn main() { atomic.int32() }",
        "fn main() { atomic.int64() }",
        "fn main() { atomic.uint32() }",
        "fn main() { atomic.uint64() }",
        "fn main() { atomic.uintptr() }",
        "fn main() { atomic.pointer() }",
        "fn main() { atomic.value() }",
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
fn atomic_arity_mismatch_rejected_e1002() {
    let cases = [
        // 1-arg functions called with 0 or 2 args
        "fn main() { atomic.load_int32() }",
        "fn main() { atomic.load_int32(1, 2) }",
        "fn main() { atomic.load_int64() }",
        "fn main() { atomic.load_int64(1, 2) }",
        "fn main() { atomic.load_uint32() }",
        "fn main() { atomic.load_uint32(1, 2) }",
        "fn main() { atomic.load_uint64() }",
        "fn main() { atomic.load_uint64(1, 2) }",
        "fn main() { atomic.load_uintptr() }",
        "fn main() { atomic.load_uintptr(1, 2) }",
        "fn main() { atomic.load_pointer() }",
        "fn main() { atomic.load_pointer(1, 2) }",
        // 2-arg functions called with 1 or 3 args
        "fn main() { atomic.store_int32(1) }",
        "fn main() { atomic.store_int32(1, 2, 3) }",
        "fn main() { atomic.store_int64(1) }",
        "fn main() { atomic.store_int64(1, 2, 3) }",
        "fn main() { atomic.add_int32(1) }",
        "fn main() { atomic.add_int32(1, 2, 3) }",
        "fn main() { atomic.add_int64(1) }",
        "fn main() { atomic.add_int64(1, 2, 3) }",
        "fn main() { atomic.swap_int32(1) }",
        "fn main() { atomic.swap_int32(1, 2, 3) }",
        "fn main() { atomic.swap_pointer(1) }",
        "fn main() { atomic.swap_pointer(1, 2, 3) }",
        // 3-arg functions called with 2 or 4 args
        "fn main() { atomic.compare_and_swap_int32(1, 2) }",
        "fn main() { atomic.compare_and_swap_int32(1, 2, 3, 4) }",
        "fn main() { atomic.compare_and_swap_int64(1, 2) }",
        "fn main() { atomic.compare_and_swap_int64(1, 2, 3, 4) }",
        "fn main() { atomic.compare_and_swap_pointer(1, 2) }",
        "fn main() { atomic.compare_and_swap_pointer(1, 2, 3, 4) }",
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
fn atomic_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [1]\n    atomic.load_int32(xs...)\n}",
        "fn main() {\n    var xs = [1, 2]\n    atomic.store_int32(xs...)\n}",
        "fn main() {\n    var xs = [1, 2, 3]\n    atomic.compare_and_swap_int32(xs...)\n}",
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
fn atomic_valid_calls_accepted() {
    let src = r#"
fn test_atomic(p *int32, p64 *int64) {
    _ = atomic.load_int32(p)
    atomic.store_int32(p, 42)
    _ = atomic.add_int32(p, 1)
    _ = atomic.swap_int32(p, 100)
    _ = atomic.compare_and_swap_int32(p, 100, 200)

    _ = atomic.load_int64(p64)
    atomic.store_int64(p64, 42)
    _ = atomic.add_int64(p64, 1)
    _ = atomic.swap_int64(p64, 100)
    _ = atomic.compare_and_swap_int64(p64, 100, 200)
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
