//! issue_3598_eprint_os_shadowing.rs: regression for #3598.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! buildable Go code. `fmt.eprintln`, `fmt.eprint`, and `fmt.eprintf` lower to
//! `fmt.Fprint*(os.Stderr, ...)` in `hy-emit`. When an in-scope parameter, local binding,
//! or package-level declaration named `os` exists, emitted Go resolves `os.Stderr` to
//! the shadowing binding, causing downstream `go build` to fail with `os.Stderr undefined`.
//!
//! `hy-check` must reject calls to `fmt.eprint*` when `os` is shadowed with `HYB-E1002`.

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
fn test_eprintln_with_parameter_os_rejects() {
    let src = r#"
import "fmt"

fn check_platform(os string) {
    if os == "unknown" {
        fmt.eprintln("unrecognized os:", os)
    }
}
"#;
    rejects(src);
}

#[test]
fn test_eprint_with_local_os_rejects() {
    let src = r#"
import "fmt"

fn test_local() {
    os := "darwin"
    fmt.eprint("error on:", os)
}
"#;
    rejects(src);
}

#[test]
fn test_eprintf_with_parameter_os_rejects() {
    let src = r#"
import "fmt"

fn test_eprintf(os int) {
    fmt.eprintf("error code: %d\n", os)
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_package_const_os_rejects() {
    let src = r#"
import "fmt"

const os = "linux"

fn test_const() {
    fmt.eprintln("error occurred")
}
"#;
    rejects(src);
}

#[test]
fn test_unshadowed_eprint_accepted() {
    let src = r#"
import "fmt"

fn main() {
    fmt.eprintln("standard error output")
    fmt.eprint("detail")
    fmt.eprintf("code: %d\n", 42)
}
"#;
    accepts(src);
}

#[test]
fn test_import_os_with_eprint_accepted() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    fmt.eprintln("standard error output")
}
"#;
    accepts(src);
}

#[test]
fn test_import_os_used_with_eprint_accepted() {
    let src = r#"
import "fmt"
import "os"

fn main() {
    _ = os.Args
    fmt.eprintln("standard error output")
}
"#;
    accepts(src);
}
