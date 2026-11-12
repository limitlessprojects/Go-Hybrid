//! shadowed_rust_bindings_and_handler_return.rs: regression tests for shadowed
//! runtime binding packages (`sha3binding`, `jsonbinding`, `regexbinding`) and
//! `http.handler` closure-return lowering (`http.HandlerFunc`).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must
//! emit buildable Go code:
//! 1. Calls to pinned Rust crate bindings (`sha3.*`, `serde_json.*`, `regex.*`)
//!    and `serde_json.value` type annotations lower to `sha3binding.*`,
//!    `jsonbinding.*`, and `regexbinding.*` in `hy-emit`. When an in-scope
//!    parameter, local binding, loop variable, type parameter, or package-level
//!    declaration shadows `sha3binding`, `jsonbinding`, or `regexbinding`,
//!    emitted Go fails downstream compilation (`undefined` / `is not a type`).
//! 2. In a function or closure whose single unnamed result is `http.handler`,
//!    `return fn(w, r) { ... }` (even without `?` in its body) lowers to
//!    `return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) { ... })`.
//!    When `http` is shadowed at the `return` site, emitted Go resolves
//!    `http.HandlerFunc` / `http.ResponseWriter` / `http.Request` to the
//!    shadowing declaration and fails `go build`.

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

fn rejects(src: &str) {
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
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn sha3_calls_with_shadowed_sha3binding_reject() {
    for src in [
        r#"
import "rust/sha3"

fn main() {
    sha3binding := 1
    _ = sha3binding
    d := sha3.digest256("hello")?
    _ = d
}
"#,
        r#"
import "rust/sha3"

fn compute(sha3binding int) (u64, error) {
    _ = sha3binding
    return sha3.stream_checksum(10, 7)
}

fn main() {}
"#,
        r#"
import "rust/sha3"

const sha3binding = 99

fn main() {
    d := sha3.digest256("hello")?
    _ = d
}
"#,
    ] {
        rejects(src);
    }
}

#[test]
fn serde_json_calls_and_value_type_with_shadowed_jsonbinding_reject() {
    for src in [
        r#"
import "rust/serde_json"

fn main() {
    jsonbinding := 1
    _ = jsonbinding
    doc := serde_json.new_object()
    _ = doc
}
"#,
        r#"
import "rust/serde_json"

const jsonbinding = "masked"

fn main() {
    doc := serde_json.parse(`{}`)?
    _ = doc
}
"#,
        r#"
import "rust/serde_json"

fn inspect(v serde_json.value) {
    _ = v
}

struct jsonbinding {
    x int
}

fn main() {}
"#,
        r#"
import "rust/serde_json"

fn main() {
    jsonbinding := 1
    _ = jsonbinding
    var v serde_json.value
    _ = v
}
"#,
    ] {
        rejects(src);
    }
}

#[test]
fn regex_calls_with_shadowed_regexbinding_reject() {
    for src in [
        r#"
import "rust/regex"

fn main() {
    regexbinding := 1
    _ = regexbinding
    ok := regex.is_valid("a+")
    _ = ok
}
"#,
        r#"
import "rust/regex"

fn check_pat(regexbinding string) (bool, error) {
    _ = regexbinding
    return regex.is_match("a+", "aaa")
}

fn main() {}
"#,
    ] {
        rejects(src);
    }
}

#[test]
fn http_handler_return_fnlit_without_try_with_shadowed_http_rejects() {
    for src in [
        r#"
import "net/http"

fn make_h(http int) http.handler {
    _ = http
    return fn(w, r) {
        _ = w
        _ = r
    }
}

fn main() {
    _ = make_h(1)
}
"#,
        r#"
import "net/http"

fn make_h() http.handler {
    http := 1
    _ = http
    return fn(w, r) {
        _ = w
        _ = r
    }
}

fn main() {
    _ = make_h()
}
"#,
        r#"
import "net/http"

fn main() {
    mk := fn() http.handler {
        http := 1
        _ = http
        return fn(w, r) {
            _ = w
            _ = r
        }
    }
    _ = mk
}
"#,
    ] {
        rejects(src);
    }
}

#[test]
fn unshadowed_rust_bindings_and_http_handler_returns_accept() {
    for src in [
        r#"
import "rust/sha3"
import "rust/serde_json"
import "rust/regex"
import "net/http"

fn make_h() http.handler {
    return fn(w, r) {
        _ = w
        _ = r
    }
}

fn main() {
    d := sha3.digest256("hello")?
    _ = d
    doc := serde_json.new_object()
    _ = doc
    ok := regex.is_valid("a+")
    _ = ok
    _ = make_h()
}
"#,
    ] {
        accepts(src);
    }
}
