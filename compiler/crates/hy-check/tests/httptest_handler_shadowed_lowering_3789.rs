//! httptest_handler_shadowed_lowering_3789.rs: regression tests for issue #3789.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! buildable Go code.
//!
//! 1. `httptest.get` and `httptest.post` lower to `http.NewRequest`, `httptest.NewRecorder`,
//!    and `strings.TrimSuffix` in `hy-emit`. When `strings` or `http` is shadowed by an
//!    in-scope parameter, local binding, loop variable, closure argument, type parameter,
//!    or package-level declaration, emitted Go fails compilation under `go build`.
//!
//! 2. `?` inside an `http.handler` closure lowers to `http.Error(w, hyErr.Error(), http.StatusBadRequest)`.
//!    When `http` or the handler's response writer parameter is shadowed by an in-scope
//!    declaration, emitted Go resolves `http.Error` to the shadowing entity or passes an
//!    invalid response writer value, failing downstream compilation.
//!
//! `hy-check` must reject these shadowed lowering invocations with `HYB-E1002`.

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

fn rejects_with_code(src: &str, code: &str, expected_substring: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {} for {:?}, got {:?}",
        code,
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains(expected_substring)),
        "expected diagnostic containing {:?} in {:?}, got {:?}",
        expected_substring,
        src,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn rejects(src: &str, expected_substring: &str) {
    rejects_with_code(src, "HYB-E1002", expected_substring);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_reproducer_1_httptest_get_with_package_const_strings_rejects() {
    let src = r#"
import "net/http"
import "net/http/httptest"

const strings = "shadowed"

fn main() error {
    mux := http.new_serve_mux()
    code, body := httptest.get(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    rejects(src, "strings` is shadowed by an in-scope declaration (HYB-E1002)");
}

#[test]
fn test_reproducer_1_httptest_get_with_local_strings_rejects() {
    let src = r#"
import "net/http"
import "net/http/httptest"

fn main() error {
    strings := 1
    _ = strings
    mux := http.new_serve_mux()
    code, body := httptest.get(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    rejects(src, "strings` is shadowed by an in-scope declaration (HYB-E1002)");
}

#[test]
fn test_reproducer_1_httptest_post_with_struct_strings_rejects() {
    let src = r#"
import "net/http"
import "net/http/httptest"

struct strings {
    val int
}

fn main() error {
    mux := http.new_serve_mux()
    code, body := httptest.post(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    rejects(src, "strings` is shadowed by an in-scope declaration (HYB-E1002)");
}

#[test]
fn test_reproducer_2_httptest_get_with_local_http_rejects() {
    let src = r#"
import "net/http"
import "net/http/httptest"

fn main() error {
    mux := http.new_serve_mux()
    http := 1
    _ = http
    code, body := httptest.get(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    rejects(src, "http` is shadowed by an in-scope declaration (HYB-E1002)");
}

#[test]
fn test_reproducer_2_httptest_post_with_local_http_rejects() {
    let src = r#"
import "net/http"
import "net/http/httptest"

fn main() error {
    mux := http.new_serve_mux()
    http := 1
    _ = http
    code, body := httptest.post(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    rejects(src, "http` is shadowed by an in-scope declaration (HYB-E1002)");
}

#[test]
fn test_reproducer_3_http_handler_with_shadowed_http_inside_closure_rejects() {
    let src = r#"
import "errors"
import "net/http"

fn step() error {
    return errors.new("boom")
}

fn main() {
    h1 := http.handler(fn(w http.response_writer, r *http.request) {
        http := 1
        _ = http
        step()?
    })
    _ = h1
}
"#;
    rejects_with_code(src, "HYB-E2011", "needs an `error` result to return through");
}

#[test]
fn test_reproducer_3_http_handler_with_shadowed_writer_in_if_block_rejects() {
    let src = r#"
import "errors"
import "net/http"

fn step() error {
    return errors.new("boom")
}

fn main() {
    h2 := http.handler(fn(w http.response_writer, r *http.request) {
        if true {
            w := 1
            _ = w
            step()?
        }
    })
    _ = h2
}
"#;
    rejects_with_code(src, "HYB-E2011", "needs an `error` result to return through");
}

#[test]
fn test_reproducer_3_http_handler_with_custom_writer_param_shadowed_rejects() {
    let src = r#"
import "errors"
import "net/http"

fn step() error {
    return errors.new("boom")
}

fn main() {
    h := http.handler(fn(resp http.response_writer, req *http.request) {
        if true {
            resp := 42
            _ = resp
            step()?
        }
    })
    _ = h
}
"#;
    rejects_with_code(src, "HYB-E2011", "needs an `error` result to return through");
}

#[test]
fn test_positive_control_unshadowed_httptest_get_accepts() {
    let src = r#"
import "net/http"
import "net/http/httptest"

fn main() error {
    mux := http.new_serve_mux()
    code, body := httptest.get(mux, "/health")?
    _ = code
    _ = body
    return nil
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_unshadowed_httptest_post_accepts() {
    let src = r#"
import "net/http"
import "net/http/httptest"

fn main() error {
    mux := http.new_serve_mux()
    code, body := httptest.post(mux, "/submit")?
    _ = code
    _ = body
    return nil
}
"#;
    accepts(src);
}

#[test]
fn test_positive_control_unshadowed_http_handler_with_try_accepts() {
    let src = r#"
import "errors"
import "net/http"

fn step() error {
    return errors.new("boom")
}

fn main() {
    h := http.handler(fn(w http.response_writer, r *http.request) {
        step()?
    })
    _ = h
}
"#;
    accepts(src);
}
