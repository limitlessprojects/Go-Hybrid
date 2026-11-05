//! issue_3598_eprint_os_shadowing.rs: regression tests for #3598.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), programs accepted by `hy-check` must emit
//! buildable Go code. `fmt.eprintln`, `fmt.eprint`, and `fmt.eprintf` lower to
//! `fmt.Fprint*(os.Stderr, ...)` in `hy-emit`. When an in-scope parameter, local binding,
//! loop variable, closure argument, type parameter, or package-level declaration named `os`
//! exists, emitted Go resolves `os.Stderr` to the shadowing entity, causing downstream
//! `go build` to fail with `os.Stderr undefined`.
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
fn test_eprintln_with_package_fn_os_rejects() {
    let src = r#"
import "fmt"

fn os() {}

fn test_fn() {
    fmt.eprintln("error occurred")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_package_struct_os_rejects() {
    let src = r#"
import "fmt"

struct os {
    version int
}

fn test_struct() {
    fmt.eprintln("error occurred")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_package_enum_os_rejects() {
    let src = r#"
import "fmt"

enum os {
    linux,
    darwin,
}

fn test_enum() {
    fmt.eprintln("error occurred")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_package_type_alias_os_rejects() {
    let src = r#"
import "fmt"

type os = string

fn test_type() {
    fmt.eprintln("error occurred")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_type_param_os_rejects() {
    let src = r#"
import "fmt"

fn test_type_param[os any]() {
    fmt.eprintln("error in generic function")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_receiver_os_rejects() {
    let src = r#"
import "fmt"

struct machine {
    id int
}

fn (os *machine) report_error() {
    fmt.eprintln("error on machine:", os.id)
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_closure_param_os_rejects() {
    let src = r#"
import "fmt"

fn test_closure() {
    cb := fn(os string) {
        fmt.eprintln("closure error:", os)
    }
    cb("linux")
}
"#;
    rejects(src);
}

#[test]
fn test_eprintln_with_for_loop_os_rejects() {
    let src = r#"
import "fmt"

fn test_loop(items []string) {
    for os in items {
        fmt.eprintln("item error:", os)
    }
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
    fmt.eprintln("status:", os.args)
}
"#;
    accepts(src);
}

#[test]
fn test_inner_scope_os_does_not_shadow_outer_eprint_accepted() {
    let src = r#"
import "fmt"

fn test_scopes() {
    {
        os := "temporary"
        fmt.println(os)
    }
    fmt.eprintln("outer scope unaffected")
}
"#;
    accepts(src);
}
