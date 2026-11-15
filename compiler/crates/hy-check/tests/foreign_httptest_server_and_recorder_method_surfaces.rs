//! Regression tests for foreign method surface gating on `httptest.server` and
//! `httptest.response_recorder` handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on standard library `net/http/httptest` handles passed
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
fn test_httptest_server_phantom_method_rejects() {
    let src = r#"
import "net/http/httptest"

struct state {
    srv httptest.server
}

fn main() {
    var s state
    s.srv.totally_fake()
}
"#;
    rejects_e1002(src, "httptest.server");
}

#[test]
fn test_httptest_response_recorder_phantom_method_rejects() {
    let src = r#"
import "net/http/httptest"

struct state {
    rec httptest.response_recorder
}

fn main() {
    var s state
    s.rec.invalid_method()
}
"#;
    rejects_e1002(src, "httptest.response_recorder");
}

#[test]
fn test_pointer_httptest_handles_reject_phantom_methods() {
    for (decl, recv_type) in [
        ("srv *httptest.server", "httptest.server"),
        ("rec *httptest.response_recorder", "httptest.response_recorder"),
    ] {
        let src = format!(
            r#"
import "net/http/httptest"

struct state {{
    {decl}
}}

fn main() {{
    var s state
    s.srv.phantom_check()
}}
"#
        );
        let src_actual = src
            .replace("s.srv.phantom_check()", if decl.starts_with("srv") {
                "s.srv.phantom_check()"
            } else {
                "s.rec.phantom_check()"
            });
        rejects_e1002(&src_actual, recv_type);
    }
}

#[test]
fn test_httptest_server_real_methods_accept() {
    let src = r#"
import "net/http/httptest"

struct state {
    srv httptest.server
}

fn main() {
    var s state
    s.srv.start()
    s.srv.start_tls()
    _ = s.srv.client()
    s.srv.close_client_connections()
    s.srv.close()
}
"#;
    accepts(src);
}

#[test]
fn test_httptest_response_recorder_real_methods_accept() {
    let src = r#"
import "net/http/httptest"

struct state {
    rec httptest.response_recorder
}

fn main() {
    var s state
    s.rec.flush()
    _ = s.rec.header()
    _ = s.rec.result()
    s.rec.write_header(200)
}
"#;
    accepts(src);
}