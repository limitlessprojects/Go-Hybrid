//! Regression tests for foreign package function calls (`http.*`).
//! Enforces call arity constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE, fixes #4163).

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
fn http_non_callable_members_rejected_e1002() {
    let cases = [
        "fn main() { http.status_ok() }",
        "fn main() { http.status_not_found() }",
        "fn main() { http.method_get() }",
        "fn main() { http.method_post() }",
        "fn main() { http.err_server_closed() }",
        "fn main() { http.default_client() }",
        "fn main() { http.default_serve_mux() }",
        "fn main() { http.no_body() }",
        "fn main() { http.client() }",
        "fn main() { http.server() }",
        "fn main() { http.request() }",
        "fn main() { http.response() }",
        "fn main() { http.header() }",
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
fn http_arity_mismatch_rejected_e1002() {
    let cases = [
        // 0-arg function called with args
        "fn main() { http.new_serve_mux(1) }",
        "fn main() { http.new_serve_mux(1, 2) }",
        // 1-arg functions called with 0 or 2 args
        "fn main() { http.get() }",
        "fn main() { http.get(\"http://example.com\", \"extra\") }",
        "fn main() { http.head() }",
        "fn main() { http.head(\"http://example.com\", \"extra\") }",
        "fn main() { http.detect_content_type() }",
        "fn main() { http.detect_content_type([1], [2]) }",
        "fn main() { http.file_server() }",
        "fn main() { http.file_server(1, 2) }",
        // 2-arg functions called with 1 or 3 args
        "fn main() { http.listen_and_serve(\":8080\") }",
        "fn main() { http.listen_and_serve(\":8080\", 1, 2) }",
        "fn main() { http.post_form(\"http://example.com\") }",
        "fn main() { http.post_form(\"http://example.com\", 1, 2) }",
        // 3-arg functions called with 2 or 4 args
        "fn main() { http.post(\"http://example.com\", \"text/plain\") }",
        "fn main() { http.post(\"http://example.com\", \"text/plain\", 1, 2) }",
        "fn main() { http.listen_and_serve_tls(\":8443\", \"cert.pem\") }",
        "fn main() { http.listen_and_serve_tls(\":8443\", \"cert.pem\", \"key.pem\", 1) }",
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
fn http_non_variadic_spread_rejected_e1002() {
    let cases = [
        "fn main() {\n    var xs = [\"http://example.com\"]\n    http.get(xs...)\n}",
        "fn main() {\n    var xs = [\":8080\", 1]\n    http.listen_and_serve(xs...)\n}",
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
fn http_valid_calls_accepted() {
    let src = r#"
fn test_http() {
    mux := http.new_serve_mux()
    _ = mux
    _ = http.detect_content_type([1, 2, 3])
}
"#;
    let r = check(src);
    assert_eq!(error_codes(&r), Vec::<String>::new());
    assert_eq!(r.verdict, Verdict::Accept);
}
