//! Regression tests for #4126:
//! Curated foreign package function calls (`strings.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types (`strings.reader`, `strings.replacer`),
//! passed `hyb check` with 0 diagnostics and subsequently failed downstream `go build`.

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
fn test_strings_package_call_too_few_arguments_rejected() {
    // strings.contains takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.contains("hello")
}
"#,
    );

    // strings.split takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.split("a,b,c")
}
"#,
    );

    // strings.clone takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.clone()
}
"#,
    );

    // strings.replace takes 4 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.replace("hello", "l", "w")
}
"#,
    );
}

#[test]
fn test_strings_package_call_too_many_arguments_rejected() {
    // strings.builder takes 0 arguments (constructor), called with 1
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.builder(1)
}
"#,
    );

    // strings.clone takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.clone("hello", "world")
}
"#,
    );

    // strings.trim_space takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.trim_space("  hello  ", "extra")
}
"#,
    );

    // strings.contains takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.contains("hello", "ll", "extra")
}
"#,
    );
}

#[test]
fn test_strings_non_callable_members_rejected() {
    // strings.reader is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.reader()
}
"#,
    );

    // strings.replacer is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "strings"

fn test() {
    _ = strings.replacer()
}
"#,
    );
}

#[test]
fn test_strings_variadic_spread_on_non_variadic_rejected() {
    // strings.contains is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "strings"

fn test(parts []string) {
    _ = strings.contains(parts...)
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

fn test(s string) {
    _ = strings.builder()
    _ = strings.clone(s)
    _ = strings.contains(s, "hello")
    _ = strings.split(s, ",")
    _ = strings.replace(s, "a", "b", 1)
    _ = strings.replace_all(s, "a", "b")
    _ = strings.trim_space(s)
    _ = strings.new_reader(s)
    _ = strings.new_replacer("a", "b", "c", "d")
}
"#,
    );
}
