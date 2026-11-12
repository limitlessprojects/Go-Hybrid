//! Soundness and buildable validation tests for `errors` package calls under Row 13 (BUILDABLE):
//! 1. Non-callable package variables (`errors.err_unsupported`) must be rejected
//!    when invoked as call targets (`HYB-E1002`).
//! 2. `errors` functions require exact arity:
//!    - 1 arg: new, unwrap.
//!    - 2 args: is.
//! 3. Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 4. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn test_errors_non_callable_variable_reject() {
    let src = "fn main() {\n    _ = errors.err_unsupported()\n}";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?} for: {}",
        codes,
        src
    );
}

#[test]
fn test_errors_one_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = errors.new()\n}",
        "fn main() {\n    _ = errors.new(\"a\", \"b\")\n}",
        "fn main() {\n    _ = errors.unwrap()\n}",
        "fn main() {\n    e := errors.new(\"x\")\n    _ = errors.unwrap(e, e)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_errors_two_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = errors.is()\n}",
        "fn main() {\n    e := errors.new(\"x\")\n    _ = errors.is(e)\n}",
        "fn main() {\n    e := errors.new(\"x\")\n    _ = errors.is(e, e, e)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for: {}", src);
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for: {}",
            codes,
            src
        );
    }
}

#[test]
fn test_errors_positive_controls_accept() {
    let cases = [
        "fn main() {\n    _ = errors.new(\"something failed\")\n}",
        "fn main() {\n    e := errors.new(\"base\")\n    _ = errors.unwrap(e)\n}",
        "fn main() {\n    e1 := errors.new(\"a\")\n    e2 := errors.new(\"b\")\n    _ = errors.is(e1, e2)\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", src);
        assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    }
}
