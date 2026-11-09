//! Regression tests for Wave 26:
//! Curated foreign package function calls (`strings.*`) must enforce call arity
//! constraints during `hyb check` (HYB-E1002, Row 13 BUILDABLE).

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

fn rejects_e1002(src: &str) {
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
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_strings_call_arity_rejected() {
    // strings.split takes 2 args, called with 0
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.split()
}
"#,
    );

    // strings.contains takes 2 args, called with 1
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.contains("hello")
}
"#,
    );

    // strings.trim_space takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.trim_space()
}
"#,
    );

    // strings.trim_space takes 1 arg, called with 2
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.trim_space("hello", "extra")
}
"#,
    );

    // strings.replace_all takes 3 args, called with 2
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.replace_all("hello world", "world")
}
"#,
    );
}

#[test]
fn test_strings_positive_controls_accepted() {
    accepts(
        r#"
package main

import "strings"

fn test() {
    _ = strings.to_lower("HELLO")
    _ = strings.trim_space("  test  ")
    _ = strings.contains("hello world", "world")
    _ = strings.has_prefix("hello world", "hello")
    _ = strings.split("a,b,c", ",")
    _ = strings.replace_all("a-b-c", "-", "_")
    buf := strings.builder()
    _ = buf
}
"#,
    );
}
