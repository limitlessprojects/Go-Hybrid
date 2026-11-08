//! Regression tests for Wave 19:
//! Curated foreign package function calls (`sort.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on these foreign package calls, invocations with too few or too many arguments,
//! or invocations on non-callable package types (`sort.interface`, `sort.int_slice`, `sort.float64_slice`, `sort.string_slice`),
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
fn test_sort_package_call_too_few_arguments_rejected() {
    // sort.ints takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    sort.ints()
}
"#,
    );

    // sort.strings takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    sort.strings()
}
"#,
    );

    // sort.float64s takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    sort.float64s()
}
"#,
    );

    // sort.search_ints takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "sort"

fn test(nums []int) {
    _ = sort.search_ints(nums)
}
"#,
    );

    // sort.search_strings takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "sort"

fn test(strs []string) {
    _ = sort.search_strings(strs)
}
"#,
    );

    // sort.search_float64s takes 2 arguments, called with 1
    rejects_e1002(
        r#"
package main

import "sort"

fn test(floats []float64) {
    _ = sort.search_float64s(floats)
}
"#,
    );
}

#[test]
fn test_sort_package_call_too_many_arguments_rejected() {
    // sort.ints takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "sort"

fn test(nums []int) {
    sort.ints(nums, 10)
}
"#,
    );

    // sort.strings takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "sort"

fn test(strs []string) {
    sort.strings(strs, "extra")
}
"#,
    );

    // sort.float64s takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "sort"

fn test(floats []float64) {
    sort.float64s(floats, 1.0)
}
"#,
    );

    // sort.search_ints takes 2 arguments, called with 3
    rejects_e1002(
        r#"
package main

import "sort"

fn test(nums []int) {
    _ = sort.search_ints(nums, 10, "extra")
}
"#,
    );
}

#[test]
fn test_sort_non_callable_members_rejected() {
    // sort.int_slice is a named slice type, not a function
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    _ = sort.int_slice()
}
"#,
    );

    // sort.float64_slice is a named slice type, not a function
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    _ = sort.float64_slice()
}
"#,
    );

    // sort.string_slice is a named slice type, not a function
    rejects_e1002(
        r#"
package main

import "sort"

fn test() {
    _ = sort.string_slice()
}
"#,
    );
}

#[test]
fn test_sort_variadic_spread_on_non_variadic_rejected() {
    // sort.ints is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "sort"

fn test(nums []int) {
    sort.ints(nums...)
}
"#,
    );

    // sort.search_ints is non-variadic; spread `...` must be rejected
    rejects_e1002(
        r#"
package main

import "sort"

fn test(nums []int) {
    _ = sort.search_ints(nums...)
}
"#,
    );
}

#[test]
fn test_sort_positive_controls_accepted() {
    accepts(
        r#"
package main

import "sort"

fn test(nums []int, strs []string, floats []float64) {
    sort.ints(nums)
    sort.strings(strs)
    sort.float64s(floats)
    _ = sort.ints_are_sorted(nums)
    _ = sort.strings_are_sorted(strs)
    _ = sort.float64s_are_sorted(floats)
    _ = sort.search_ints(nums, 42)
    _ = sort.search_strings(strs, "target")
    _ = sort.search_float64s(floats, 3.14)
}
"#,
    );
}
