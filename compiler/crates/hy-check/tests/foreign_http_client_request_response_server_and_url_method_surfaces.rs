//! Regression tests for foreign method surface gating on `http.client`, `http.request`,
//! `http.response`, `http.server`, `http.cookie`, and `url.url` handles
//! (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these standard library HTTP and URL handles passed
//! `hyb check` with 0 diagnostics and died at `go build` with
//! `type ... has no field or method ...`.

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

fn rejects_e1002(src: &str, expected_substring: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "must emit HYB-E1002, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains(expected_substring)),
        "diagnostic message must contain {:?}, got: {:?}",
        expected_substring,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_http_client_phantom_method_rejects() {
    let src = r#"
import "net/http"

struct state {
    client http.client
}

fn main() {
    var s state
    s.client.totally_fake()
}
"#;
    rejects_e1002(src, "http.client");
}

#[test]
fn test_http_request_phantom_method_rejects() {
    let src = r#"
import "net/http"

struct state {
    req http.request
}

fn main() {
    var s state
    s.req.invalid_call()
}
"#;
    rejects_e1002(src, "http.request");
}

#[test]
fn test_http_response_phantom_method_rejects() {
    let src = r#"
import "net/http"

struct state {
    resp http.response
}

fn main() {
    var s state
    s.resp.bogus_method()
}
"#;
    rejects_e1002(src, "http.response");
}

#[test]
fn test_http_server_phantom_method_rejects() {
    let src = r#"
import "net/http"

struct state {
    srv http.server
}

fn main() {
    var s state
    s.srv.bad_action()
}
"#;
    rejects_e1002(src, "http.server");
}

#[test]
fn test_http_cookie_phantom_method_rejects() {
    let src = r#"
import "net/http"

struct state {
    c http.cookie
}

fn main() {
    var s state
    s.c.fake_cookie_method()
}
"#;
    rejects_e1002(src, "http.cookie");
}

#[test]
fn test_url_url_phantom_method_rejects() {
    let src = r#"
import "net/url"

struct state {
    u url.url
}

fn main() {
    var s state
    s.u.unsupported_url_call()
}
"#;
    rejects_e1002(src, "url.url");
}

#[test]
fn test_http_request_basic_auth_methods_accept() {
    // `net/http.Request` ships BasicAuth and SetBasicAuth. Omitting them from
    // the curated surface made the checker reject valid Go with a diagnostic
    // claiming Go has no such method — a false rejection, the mirror image of
    // the phantom-method bug this surface was built to stop.
    let src = r#"
import "net/http"

struct state {
    req http.request
}

fn main() {
    var s state
    s.req.set_basic_auth("user", "pass")
    u, p, ok := s.req.basic_auth()
    fmt.println("{u} {p} {ok}")
}
"#;
    accepts(src);
}

#[test]
fn test_real_methods_on_http_and_url_handles_accept() {
    let src = r#"
import "net/http"
import "net/url"

struct state {
    client http.client
    req    http.request
    resp   http.response
    srv    http.server
    cookie http.cookie
    u      url.url
}

fn main() {
    var s state
    s.client.close_idle_connections()
    _ = s.req.user_agent()
    _ = s.req.proto_at_least(1, 1)
    _ = s.resp.proto_at_least(1, 1)
    _ = s.srv.close()
    _ = s.cookie.string()
    _ = s.u.hostname()
    _ = s.u.is_abs()
}
"#;
    accepts(src);
}