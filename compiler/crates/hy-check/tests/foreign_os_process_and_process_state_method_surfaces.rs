//! Regression tests for foreign method surface gating on `os.process` and
//! `os.process_state` handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these standard library process handles passed
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
fn test_os_process_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    proc os.process
}

fn main() {
    var s state
    s.proc.totally_fake()
}
"#;
    rejects_e1002(src, "os.process");
}

#[test]
fn test_os_process_state_phantom_method_rejects() {
    let src = r#"
import "os"

struct state {
    ps os.process_state
}

fn main() {
    var s state
    s.ps.totally_fake()
}
"#;
    rejects_e1002(src, "os.process_state");
}

#[test]
fn test_os_process_invalid_method_rejects() {
    let src = r#"
import "os"

struct state {
    proc os.process
}

fn main() {
    var s state
    s.proc.terminate_now()
}
"#;
    rejects_e1002(src, "os.process");
}

#[test]
fn test_os_process_state_invalid_method_rejects() {
    let src = r#"
import "os"

struct state {
    ps os.process_state
}

fn main() {
    var s state
    s.ps.dump_stack()
}
"#;
    rejects_e1002(src, "os.process_state");
}

#[test]
fn test_real_methods_on_os_process_and_process_state_handles_accept() {
    let src = r#"
import "os"

struct state {
    proc os.process
    ps   os.process_state
}

fn main() {
    var s state
    _ = s.proc.kill()
    _ = s.proc.release()
    _ = s.proc.wait()
    _ = s.ps.exit_code()
    _ = s.ps.exited()
    _ = s.ps.pid()
    _ = s.ps.string()
    _ = s.ps.success()
    _ = s.ps.system_time()
    _ = s.ps.user_time()
}
"#;
    accepts(src);
}
