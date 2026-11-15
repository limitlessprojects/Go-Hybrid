//! Regression tests for foreign method surface gating on `strings.reader`
//! handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on `strings.reader` passed
//! `hyb check` with 0 diagnostics and died at `go build` with
//! `s.r.TotallyFake undefined (type strings.Reader has no field or method TotallyFake)`.

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
fn test_strings_reader_phantom_methods_reject() {
    let src = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    s.r.totally_fake()
}
"#;
    rejects_e1002(src, "strings.reader");
}

#[test]
fn test_strings_reader_pointer_receiver_phantom_rejects() {
    let src = r#"
import "strings"

fn inspect(r *strings.reader) {
    r.bogus_method()
}
"#;
    rejects_e1002(src, "strings.reader");
}

#[test]
fn test_strings_reader_non_existent_stream_methods_reject() {
    // strings.Reader is read-only; methods like close, write, string belong to other types.
    let src_close = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    s.r.close()
}
"#;
    rejects_e1002(src_close, "strings.reader");

    let src_write = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    s.r.write("data")
}
"#;
    rejects_e1002(src_write, "strings.reader");

    let src_string = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    s.r.string()
}
"#;
    rejects_e1002(src_string, "strings.reader");
}

#[test]
fn test_strings_reader_additional_phantom_rejects() {
    let src = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    s.r.phantom_call()
}
"#;
    rejects_e1002(src, "strings.reader");
}

#[test]
fn test_strings_reader_valid_methods_accept() {
    let src = r#"
import "strings"

struct state {
    r strings.reader
}

fn main() {
    var s state
    _ = s.r.len()
    _ = s.r.size()
    s.r.reset("hello")
}
"#;
    accepts(src);
}

#[test]
fn test_strings_reader_pointer_valid_methods_accept() {
    let src = r#"
import "strings"

fn process(r *strings.reader) {
    _ = r.len()
    _ = r.size()
    r.reset("sample")
}
"#;
    accepts(src);
}
