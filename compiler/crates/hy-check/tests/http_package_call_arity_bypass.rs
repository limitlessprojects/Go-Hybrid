//! Regression tests for Wave 23:
//! Curated foreign package function calls (`net/http.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types/variables/constants (`http.status_ok`, `http.method_get`, `http.client`, `http.request`, `http.err_server_closed`),
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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_http_package_call_too_few_arguments_rejected() {
    // http.get takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _, _ = http.get()
}
"#,
    );

    // http.post takes 3 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _, _ = http.post("http://example.com")
}
"#,
    );

    // http.listen_and_serve takes 2 arguments, called with 0
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.listen_and_serve()
}
"#,
    );

    // http.status_text takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.status_text()
}
"#,
    );
}

#[test]
fn test_http_package_call_too_many_arguments_rejected() {
    // http.get takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _, _ = http.get("http://example.com", "extra")
}
"#,
    );

    // http.status_text takes 1 argument, called with 3
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.status_text(200, 300, 400)
}
"#,
    );
}

#[test]
fn test_http_non_callable_members_rejected() {
    // Status constants
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.status_ok()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.status_not_found()
}
"#,
    );

    // Method constants
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.method_get()
}
"#,
    );

    // Default variables
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.default_client()
}
"#,
    );

    // Error variables
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.err_server_closed()
}
"#,
    );

    // Struct / Interface types
    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.client()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.request()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "net/http"

fn test() {
    _ = http.response()
}
"#,
    );
}

#[test]
fn test_http_positive_controls_accepted() {
    accepts(
        r#"
package main

import "net/http"

fn test() {
    _, _ = http.get("http://example.com")
    _ = http.status_text(200)
    _ = http.detect_content_type("<html></html>")
}
"#,
    );
}
