//! Regression tests for foreign method surface gating on `time.location`,
//! `time.parse_error`, `strconv.num_error`, `os.path_error`, `os.link_error`,
//! and `os.syscall_error` (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under `GUARANTEES.md` Row 13 (`BUILDABLE`), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these curated standard library types
//! passed `hyb check` with 0 diagnostics and failed at `go build` with
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
fn test_time_location_phantom_methods_reject() {
    let src = r#"
import "time"

struct state {
    loc time.location
}

fn main() {
    var s state
    s.loc.totally_fake()
}
"#;
    rejects_e1002(src, "time.location");
}

#[test]
fn test_time_fixed_zone_local_phantom_methods_reject() {
    let src = r#"
import "time"

fn main() {
    loc := time.fixed_zone("UTC+1", 3600)
    loc.reset()
}
"#;
    rejects_e1002(src, "time.location");
}

#[test]
fn test_time_location_valid_methods_accept() {
    let src = r#"
import "time"

fn main() {
    loc := time.fixed_zone("UTC+1", 3600)
    name := loc.string()
    _ = name
}
"#;
    accepts(src);
}

#[test]
fn test_time_parse_error_phantom_methods_reject() {
    let src = r#"
import "time"

struct state {
    pe time.parse_error
}

fn main() {
    var s state
    s.pe.totally_fake()
}
"#;
    rejects_e1002(src, "time.parse_error");
}

#[test]
fn test_strconv_num_error_phantom_methods_reject() {
    let src = r#"
import "strconv"

struct state {
    ne strconv.num_error
}

fn main() {
    var s state
    s.ne.totally_fake()
}
"#;
    rejects_e1002(src, "strconv.num_error");
}

#[test]
fn test_strconv_num_error_valid_methods_accept() {
    let src = r#"
import "strconv"

struct state {
    ne strconv.num_error
}

fn main() {
    var s state
    msg := s.ne.error()
    inner := s.ne.unwrap()
    _ = msg
    _ = inner
}
"#;
    accepts(src);
}

#[test]
fn test_os_error_types_phantom_methods_reject() {
    let src = r#"
import "os"

struct state {
    pe os.path_error
    le os.link_error
    se os.syscall_error
}

fn main() {
    var s state
    s.pe.totally_fake()
}
"#;
    rejects_e1002(src, "os.path_error");
}

#[test]
fn test_os_error_types_valid_methods_accept() {
    let src = r#"
import "os"

struct state {
    pe os.path_error
    le os.link_error
    se os.syscall_error
}

fn main() {
    var s state
    _ = s.pe.error()
    _ = s.pe.timeout()
    _ = s.pe.unwrap()
    _ = s.le.error()
    _ = s.le.unwrap()
    _ = s.se.error()
    _ = s.se.timeout()
    _ = s.se.unwrap()
}
"#;
    accepts(src);
}
