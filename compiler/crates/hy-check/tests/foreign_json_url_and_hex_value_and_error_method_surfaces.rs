//! Regression tests for foreign method surface gating on `json.number`,
//! `json.delim`, `json.syntax_error`, `json.unmarshal_type_error`,
//! `json.unsupported_type_error`, `json.unsupported_value_error`,
//! `json.marshaler_error`, `url.error`, `url.escape_error`,
//! `url.invalid_host_error`, and `hex.invalid_byte_error`
//! (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under `GUARANTEES.md` Row 13 (`BUILDABLE`), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these curated `json`, `url`, and `hex`
//! types passed `hyb check` with 0 diagnostics and failed at `go build` with
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
fn test_json_number_and_delim_phantom_methods_reject() {
    let src = r#"
import "encoding/json"

struct state {
    num json.number
}

fn main() {
    var s state
    s.num.totally_fake()
}
"#;
    rejects_e1002(src, "json.number");
}

#[test]
fn test_json_number_and_delim_valid_methods_accept() {
    let src = r#"
import "encoding/json"

struct state {
    num json.number
    del json.delim
}

fn main() {
    var s state
    _ = s.num.string()
    s.num.int64()
    s.num.float64()
    _ = s.del.string()
}
"#;
    accepts(src);
}

#[test]
fn test_json_error_types_phantom_methods_reject() {
    let src = r#"
import "encoding/json"

struct state {
    se json.syntax_error
}

fn main() {
    var s state
    s.se.totally_fake()
}
"#;
    rejects_e1002(src, "json.syntax_error");
}

#[test]
fn test_json_error_types_valid_methods_accept() {
    let src = r#"
import "encoding/json"

struct state {
    se json.syntax_error
    ute json.unmarshal_type_error
    uste json.unsupported_type_error
    usve json.unsupported_value_error
    me json.marshaler_error
}

fn main() {
    var s state
    _ = s.se.error()
    _ = s.ute.error()
    _ = s.uste.error()
    _ = s.usve.error()
    _ = s.me.error()
    _ = s.me.unwrap()
}
"#;
    accepts(src);
}

#[test]
fn test_url_and_hex_error_types_phantom_methods_reject() {
    let src = r#"
import "net/url"

struct state {
    ue url.error
}

fn main() {
    var s state
    s.ue.totally_fake()
}
"#;
    rejects_e1002(src, "url.error");
}

#[test]
fn test_url_and_hex_error_types_valid_methods_accept() {
    let src = r#"
import "net/url"
import "encoding/hex"

struct state {
    ue url.error
    ee url.escape_error
    ihe url.invalid_host_error
    ibe hex.invalid_byte_error
}

fn main() {
    var s state
    _ = s.ue.error()
    _ = s.ue.temporary()
    _ = s.ue.timeout()
    _ = s.ue.unwrap()
    _ = s.ee.error()
    _ = s.ihe.error()
    _ = s.ibe.error()
}
"#;
    accepts(src);
}
