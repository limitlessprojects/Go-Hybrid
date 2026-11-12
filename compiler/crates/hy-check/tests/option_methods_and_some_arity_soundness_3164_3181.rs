//! option_methods_and_some_arity_soundness_3164_3181.rs: regression tests for #3164 and #3181.
//!
//! Under Go-Hybrid specification and GUARANTEES.md Row 13 (BUILDABLE):
//! - Built-in `some(v)` is the prelude constructor function that wraps an expression
//!   of type `T` into an `option[T]`. It requires exactly one argument without variadic spread `...`.
//! - `option[T]` methods `unwrap()`, `is_some()`, and `is_none()` require 0 arguments.
//! - `option[T].or(default)` requires exactly 1 argument matching the option payload type `T`.
//!
//! Verify that invalid argument counts and variadic spread on `some()` and `option[T]`
//! methods are rejected with HYB-E1002, while valid usage continues to accept.

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
        cs.iter().any(|c| c == "HYB-E1002" || c == "HYB-E2005"),
        "expected error diagnostic for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
}

// ---------------------------------------------------------------- reject built-in some() arity & spread (#3181):

#[test]
fn some_with_zero_args_rejects() {
    rejects(
        r#"
fn main() {
    _ = some()
}
"#,
    );
}

#[test]
fn some_with_multiple_args_rejects() {
    rejects(
        r#"
fn main() {
    _ = some(1, 2)
}
"#,
    );
}

#[test]
fn some_with_three_args_rejects() {
    rejects(
        r#"
fn main() {
    _ = some(1, 2, 3)
}
"#,
    );
}

#[test]
fn some_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []int{1, 2}
    _ = some(xs...)
}
"#,
    );
}

// ---------------------------------------------------------------- reject option[T] methods arity (#3164):

#[test]
fn option_unwrap_with_arg_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.unwrap(123)
}
"#,
    );
}

#[test]
fn option_is_some_with_arg_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.is_some(true)
}
"#,
    );
}

#[test]
fn option_is_none_with_arg_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.is_none("extra")
}
"#,
    );
}

#[test]
fn option_or_with_zero_args_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.or()
}
"#,
    );
}

#[test]
fn option_or_with_multiple_args_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.or(1, 2)
}
"#,
    );
}

#[test]
fn option_or_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    xs := []int{0}
    _ = o.or(xs...)
}
"#,
    );
}

#[test]
fn option_or_with_mismatched_type_rejects() {
    rejects(
        r#"
fn main() {
    var o option[int]
    _ = o.or("not_an_int")
}
"#,
    );
}

// ---------------------------------------------------------------- positive controls (valid usage):

#[test]
fn valid_some_accepted() {
    accepts(
        r#"
fn main() {
    _ = some(42)
    _ = some("hello")
    _ = some(true)
}
"#,
    );
}

#[test]
fn valid_option_methods_accepted() {
    accepts(
        r#"
fn main() {
    var o option[int]
    _ = o.unwrap()
    _ = o.is_some()
    _ = o.is_none()
    _ = o.or(10)
}
"#,
    );
}
