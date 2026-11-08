//! Regression tests for #4128 / Wave 18:
//! Curated foreign package function calls (`strconv.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package members (`strconv.int_size`, `strconv.err_range`, `strconv.err_syntax`, `strconv.num_error`),
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
fn test_strconv_package_call_too_few_arguments_rejected() {
    // strconv.atoi takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.atoi()
}
"#,
    );

    // strconv.itoa takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.itoa()
}
"#,
    );

    // strconv.parse_bool takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.parse_bool()
}
"#,
    );

    // strconv.parse_int takes 3 arguments, called with 2
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.parse_int("123", 10)
}
"#,
    );

    // strconv.format_float takes 4 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.format_float(3.14, 'f', 2)
}
"#,
    );

    // strconv.append_float takes 5 arguments, called with 4
    rejects_e1002(
        r#"
package main

import "strconv"

fn test(buf []byte) {
    _ = strconv.append_float(buf, 3.14, 'f', 2)
}
"#,
    );
}

#[test]
fn test_strconv_package_call_too_many_arguments_rejected() {
    // strconv.atoi takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.atoi("123", 10)
}
"#,
    );

    // strconv.itoa takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.itoa(42, 10)
}
"#,
    );

    // strconv.quote takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.quote("hello", "extra")
}
"#,
    );

    // strconv.unquote_char takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.unquote_char("a", '"', "extra")
}
"#,
    );
}

#[test]
fn test_strconv_non_callable_members_rejected() {
    // strconv.int_size is a constant int, not a function
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.int_size()
}
"#,
    );

    // strconv.err_range is a package error variable, not a function
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.err_range()
}
"#,
    );

    // strconv.err_syntax is a package error variable, not a function
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.err_syntax()
}
"#,
    );

    // strconv.num_error is a struct type, not a function
    rejects_e1002(
        r#"
package main

import "strconv"

fn test() {
    _ = strconv.num_error()
}
"#,
    );
}

#[test]
fn test_strconv_variadic_spread_on_non_variadic_rejected() {
    // strconv.atoi is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "strconv"

fn test(args []string) {
    _ = strconv.atoi(args...)
}
"#,
    );

    // strconv.format_int is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "strconv"

fn test(args []int64) {
    _ = strconv.format_int(args...)
}
"#,
    );
}

#[test]
fn test_strconv_positive_controls_accepted() {
    accepts(
        r#"
package main

import "strconv"

fn test(buf []byte) {
    _, _ = strconv.atoi("123")
    _ = strconv.itoa(42)
    _, _ = strconv.parse_bool("true")
    _, _ = strconv.parse_int("123", 10, 64)
    _, _ = strconv.parse_uint("123", 10, 64)
    _ = strconv.format_bool(true)
    _ = strconv.format_int(42, 10)
    _ = strconv.format_uint(42, 10)
    _ = strconv.format_float(3.14, 'f', 2, 64)
    _ = strconv.append_bool(buf, true)
    _ = strconv.append_int(buf, 42, 10)
    _ = strconv.quote("hello")
    _, _ = strconv.unquote("\"hello\"")
}
"#,
    );
}
