//! package_value_position.rs - tests for HYB-E1002 rejection of
//! bare package roots used in value positions (#2358).
//!
//! Go syntax / semantic rules:
//! A package identifier may only be used as a qualifier before a selector (`pkg.Member`).
//! Using a package name in value position fails go build with
//! 'use of package X not in selector' (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn bare_builtin_package_as_arg_rejects_e1002() {
    let src = r#"
fn main() {
    fmt.println(fmt)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("use of package `fmt` not in selector")),
        "expected package selector message in {:?}",
        r.diagnostics
    );
}

#[test]
fn imported_package_as_value_assignment_rejects_e1002() {
    let src = r#"
import "strings"

fn main() {
    fmt.println(strings.to_upper("x"))
    y := strings
    fmt.println(y)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d
            .message
            .contains("use of package `strings` not in selector")),
        "expected package selector message in {:?}",
        r.diagnostics
    );
}

#[test]
fn bare_package_in_binary_op_rejects_e1002() {
    let src = r#"
fn main() {
    _ = fmt + 1
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn bare_package_in_index_rejects_e1002() {
    let src = r#"
fn main() {
    _ = fmt[0]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn bare_package_in_addrof_rejects_e1002() {
    let src = r#"
fn main() {
    _ = &fmt
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn bare_package_in_interp_rejects_e1002() {
    let src = r#"
fn main() {
    _ = "{fmt}"
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {:?}",
        r.diagnostics
    );
}

#[test]
fn qualifier_position_accepted() {
    let src = r#"
fn main() {
    fmt.println(fmt.sprint("x"))
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "qualifier position should produce 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn local_shadow_inside_block_accepted() {
    let src = r#"
import "strings"

fn main() {
    fmt.println(strings.to_upper("x"))
    {
        strings := 1
        fmt.println(strings)
    }
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "local shadow in block should produce 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn package_scope_fn_shadows_root_accepted() {
    let src = r#"
fn path() int {
    return 1
}

fn main() {
    _ = path()
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "fn shadowing package root should produce 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn parameter_shadows_package_root_accepted() {
    let src = r#"
import "strings"

fn helper(strings string) string {
    return strings
}

fn main() {
    fmt.println(strings.to_upper(helper("a")))
}
"#;
    let r = check(src);
    assert!(
        r.diagnostics.is_empty(),
        "parameter shadowing package root should produce 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
