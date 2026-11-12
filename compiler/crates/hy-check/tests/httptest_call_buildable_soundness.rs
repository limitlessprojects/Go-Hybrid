//! Soundness and buildable validation tests for `net/http/httptest` (`httptest`) calls under Row 13 (BUILDABLE):
//! 1. Non-callable package types and constants (`httptest.response_recorder`, `httptest.server`,
//!    `httptest.default_remote_addr`) must be rejected when invoked as call targets (`HYB-E1002`).
//! 2. `httptest` functions require exact arity:
//!    - 0 args: new_recorder.
//!    - 1 arg: new_server, new_tls_server, new_unstarted_server.
//!    - 2 args: get, post.
//!    - 3 args: new_request.
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
fn test_httptest_non_callable_symbols_reject() {
    let cases = [
        "fn main() {\n    _ = httptest.server()\n}",
        "fn main() {\n    _ = httptest.response_recorder()\n}",
        "fn main() {\n    _ = httptest.default_remote_addr()\n}",
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
fn test_httptest_zero_arg_function_arity_reject() {
    let cases = [
        "fn main() {\n    _ = httptest.new_recorder(1)\n}",
        "fn main() {\n    _ = httptest.new_recorder(\"w\", 2)\n}",
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
fn test_httptest_one_arg_functions_arity_reject() {
    let cases = [
        "fn main() {\n    _ = httptest.new_server()\n}",
        "fn main() {\n    _ = httptest.new_server(1, 2)\n}",
        "fn main() {\n    _ = httptest.new_tls_server()\n}",
        "fn main() {\n    _ = httptest.new_tls_server(1, 2)\n}",
        "fn main() {\n    _ = httptest.new_unstarted_server()\n}",
        "fn main() {\n    _ = httptest.new_unstarted_server(1, 2)\n}",
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
fn test_httptest_three_arg_function_arity_reject() {
    let cases = [
        "fn main() {\n    _ = httptest.new_request()\n}",
        "fn main() {\n    _ = httptest.new_request(\"GET\")\n}",
        "fn main() {\n    _ = httptest.new_request(\"GET\", \"/path\")\n}",
        "fn main() {\n    _ = httptest.new_request(\"GET\", \"/path\", \"body\", 4)\n}",
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
fn test_httptest_positive_controls_accept() {
    let cases = [
        "fn main() {\n    _ = httptest.new_recorder()\n}",
        "fn main() {\n    _ = httptest.new_server(\"handler\")\n}",
        "fn main() {\n    _ = httptest.new_tls_server(\"handler\")\n}",
        "fn main() {\n    _ = httptest.new_unstarted_server(\"handler\")\n}",
        "fn main() {\n    _ = httptest.new_request(\"GET\", \"/api/test\", \"body\")\n}",
    ];

    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for: {}", src);
        assert!(r.diagnostics.is_empty(), "unexpected diagnostics: {:?}", r.diagnostics);
    }
}
