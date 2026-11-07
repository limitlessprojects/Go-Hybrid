//! Regression tests for foreign method surface gating on `bytes.reader` / `bytes.Reader`
//! and curated `"bytes"` package member/result resolution (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under `GUARANTEES.md` Row 13 (`BUILDABLE`), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! PR #3909 added `"bytes"` to `is_curated_package_root` and `BYTES_BUFFER` to
//! `foreign_method_surface`, but omitted `bytes.reader` / `bytes.Reader` from
//! `foreign_method_surface`, `"buffer" | "reader"` from `is_curated_package_member("bytes", ...)`,
//! and `bytes.new_reader` / `bytes.new_buffer` from `package_mode` / `package_result`.

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
fn test_bytes_reader_struct_field_phantom_methods_reject() {
    let src = r#"
import "bytes"

struct state {
    r bytes.reader
}

fn main() {
    var s state
    s.r.totally_fake()
}
"#;
    rejects_e1002(src, "bytes.reader");
}

#[test]
fn test_bytes_reader_go_spelling_phantom_methods_reject() {
    let src = r#"
import "bytes"

struct state {
    r bytes.Reader
}

fn main() {
    var s state
    s.r.non_existent_method()
}
"#;
    rejects_e1002(src, "bytes.Reader");
}

#[test]
fn test_bytes_new_reader_local_phantom_methods_reject() {
    let src = r#"
import "bytes"

fn main() {
    var data []byte
    r := bytes.new_reader(data)
    r.close()
}
"#;
    rejects_e1002(src, "bytes.reader");
}

#[test]
fn test_bytes_new_buffer_local_phantom_methods_reject() {
    let src = r#"
import "bytes"

fn main() {
    b := bytes.new_buffer_string("hello")
    b.totally_fake()
}
"#;
    rejects_e1002(src, "bytes.buffer");
}

#[test]
fn test_bytes_reader_valid_methods_accept() {
    let src = r#"
import "bytes"

struct state {
    r bytes.reader
}

fn main() {
    var s state
    var buf []byte
    _ = s.r.len()
    _ = s.r.size()
    s.r.reset(buf)
}
"#;
    accepts(src);
}

#[test]
fn test_bytes_new_reader_valid_methods_accept() {
    let src = r#"
import "bytes"

fn main() {
    var data []byte
    r := bytes.new_reader(data)
    _ = r.len()
    _ = r.size()
    r.reset(data)
}
"#;
    accepts(src);
}
