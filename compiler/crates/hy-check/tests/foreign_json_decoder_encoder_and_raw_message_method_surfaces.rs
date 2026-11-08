//! Regression tests for foreign method surface gating on `json.decoder`, `json.encoder`,
//! and `json.raw_message` handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on standard library `encoding/json` handles passed
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
fn test_json_decoder_phantom_method_rejects() {
    let src = r#"
import "encoding/json"

struct state {
    dec json.decoder
}

fn main() {
    var s state
    s.dec.totally_fake()
}
"#;
    rejects_e1002(src, "json.decoder");
}

#[test]
fn test_json_encoder_phantom_method_rejects() {
    let src = r#"
import "encoding/json"

struct state {
    enc json.encoder
}

fn main() {
    var s state
    s.enc.invalid_method()
}
"#;
    rejects_e1002(src, "json.encoder");
}

#[test]
fn test_json_raw_message_phantom_method_rejects() {
    let src = r#"
import "encoding/json"

struct state {
    raw json.raw_message
}

fn main() {
    var s state
    s.raw.bogus_call()
}
"#;
    rejects_e1002(src, "json.raw_message");
}

#[test]
fn test_pointer_json_handles_reject_phantom_methods() {
    for (decl, recv_type) in [
        ("dec *json.decoder", "json.decoder"),
        ("enc *json.encoder", "json.encoder"),
        ("raw *json.raw_message", "json.raw_message"),
    ] {
        let src = format!(
            r#"
import "encoding/json"

struct state {{
    {decl}
}}

fn main() {{
    var s state
    s.dec.phantom_check()
}}
"#
        );
        let src_actual = src
            .replace("s.dec.phantom_check()", if decl.starts_with("dec") {
                "s.dec.phantom_check()"
            } else if decl.starts_with("enc") {
                "s.enc.phantom_check()"
            } else {
                "s.raw.phantom_check()"
            });
        rejects_e1002(&src_actual, recv_type);
    }
}

#[test]
fn test_json_decoder_real_methods_accept() {
    let src = r#"
import "encoding/json"

struct state {
    dec json.decoder
}

fn main() {
    var s state
    s.dec.disallow_unknown_fields()
    s.dec.use_number()
    _ = s.dec.more()
    _ = s.dec.input_offset()
    _ = s.dec.buffered()
}
"#;
    accepts(src);
}

#[test]
fn test_json_encoder_real_methods_accept() {
    let src = r#"
import "encoding/json"

struct state {
    enc json.encoder
}

fn main() {
    var s state
    s.enc.set_escape_html(true)
    s.enc.set_indent("", "  ")
}
"#;
    accepts(src);
}

#[test]
fn test_json_raw_message_real_methods_accept() {
    let src = r#"
import "encoding/json"

struct state {
    raw json.raw_message
}

fn main() {
    var s state
    _ = s.raw.marshal_json()
}
"#;
    accepts(src);
}
