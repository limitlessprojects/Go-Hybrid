//! Regression tests for #3598 (GUARANTEES Row 13 BUILDABLE):
//! `fmt.eprintln(...)`, `fmt.eprint(...)`, and `fmt.eprintf(...)` lower to
//! `fmt.Fprintln(os.Stderr, ...)`, `fmt.Fprint(os.Stderr, ...)`, and
//! `fmt.Fprintf(os.Stderr, ...)` in `hy-emit`. When `os` is shadowed by an
//! in-scope parameter, local binding, or package-level constant, emitted Go
//! resolves `os.Stderr` to the local binding and `go build` fails with
//! `os.Stderr undefined`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn has_e1002_shadowed_os(r: &CheckResult) -> bool {
    r.diagnostics.iter().any(|d| {
        d.severity == Severity::Error
            && d.code == "HYB-E1002"
            && (d.message.contains("shadowed") || d.message.contains("Stderr"))
    })
}

#[test]
fn test_eprintln_param_shadowed_os_rejected() {
    let src = r#"
import "fmt"

fn check_platform(os string) {
    fmt.eprintln("error on platform:", os)
}

fn main() {
    check_platform("linux")
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprintln with shadowed parameter `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprint_param_shadowed_os_rejected() {
    let src = r#"
import "fmt"

fn write_err(os string) {
    fmt.eprint("error:", os)
}

fn main() {
    write_err("darwin")
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprint with shadowed parameter `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprintf_param_shadowed_os_rejected() {
    let src = r#"
import "fmt"

fn log_fmt(os string) {
    fmt.eprintf("error: %s
", os)
}

fn main() {
    log_fmt("freebsd")
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprintf with shadowed parameter `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprintln_local_shadowed_os_rejected() {
    let src = r#"
import "fmt"

fn main() {
    os := "windows"
    fmt.eprintln("running on:", os)
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprintln with local `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprintln_pkg_const_shadowed_os_rejected() {
    let src = r#"
import "fmt"

const os = "custom_os"

fn main() {
    fmt.eprintln("fatal error")
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprintln with package-level const `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprintln_closure_shadowed_os_rejected() {
    let src = r#"
import "fmt"

fn main() {
    logger := fn(os string) {
        fmt.eprintln("log:", os)
    }
    logger("netbsd")
}
"#;
    let r = check(src);
    assert!(
        has_e1002_shadowed_os(&r),
        "expected HYB-E1002 for fmt.eprintln with closure parameter `os`, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn test_eprintln_unshadowed_os_accepted() {
    let src = r#"
import "fmt"

fn report(platform string) {
    fmt.eprintln("platform:", platform)
}

fn main() {
    report("linux")
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "unshadowed fmt.eprintln must be accepted with 0 diagnostics, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_println_with_param_os_accepted() {
    let src = r#"
import "fmt"

fn log_info(os string) {
    fmt.println("info on:", os)
}

fn main() {
    log_info("linux")
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "fmt.println with param os must be accepted with 0 diagnostics, got {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
