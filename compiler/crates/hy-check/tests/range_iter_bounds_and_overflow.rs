//! Regression tests for HYB-E2009 on `for i in lo..hi` / `for i in lo..=hi`:
//! 1. `nil` bounds (`for i in 0..nil`, `for i in nil..5`).
//! 2. Untyped `const` float / bool / string identifiers (file-level and block-scoped `const`).
//! 3. Boolean comparison / logical expressions and `!` / `&` unary bounds (`for i in 0..(1 < 2)`,
//!    `for i in (!b)..5`, `for i in 0..(&x)`).
//! 4. Mismatched integer types between `lo` and `hi` (`var n u32 = 5; for i in 0..n`,
//!    `var lo i32 = 0; var hi i64 = 5; for i in lo..hi`, inclusive `var lo i64 = 0; for i in lo..=10`).
//! 5. Constant integer bounds that overflow the loop counter's integer type (`for i in 0..(1 << 63)`,
//!    `var lo i8 = 0; for i in lo..300`, `var hi u8 = 10; for i in -1..hi`).

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

fn rejects_with_e2009(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e2009(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2009"),
        "must NOT fire HYB-E2009 for {:?}, got {:?}",
        src,
        cs
    );
}

#[test]
fn nil_and_boolean_or_pointer_range_bounds_reject_e2009() {
    rejects_with_e2009("fn main() { for i in 0..nil { _ = i } }");
    rejects_with_e2009("fn main() { for i in nil..5 { _ = i } }");
    rejects_with_e2009("fn main() { for i in 0..(1 < 2) { _ = i } }");
    rejects_with_e2009("fn main() { for i in (1 == 1)..5 { _ = i } }");
    rejects_with_e2009("fn main() { b := false\nfor i in (!b)..5 { _ = i } }");
    rejects_with_e2009("fn main() { x := 1\nfor i in 0..(&x) { _ = i } }");
}

#[test]
fn const_float_bool_string_range_bounds_reject_e2009() {
    rejects_with_e2009("const f = 2.5\nfn main() { for i in 0..f { _ = i } }");
    rejects_with_e2009("const b = true\nfn main() { for i in 0..b { _ = i } }");
    rejects_with_e2009("const s = \"abc\"\nfn main() { for i in 0..s { _ = i } }");
    rejects_with_e2009("fn main() { const f = 2.5\nfor i in 0..f { _ = i } }");
    rejects_with_e2009("fn main() { const b = true\nfor i in b..5 { _ = i } }");
    rejects_with_e2009("fn main() { fn() { const f = 2.5\nfor i in 0..f { _ = i } }() }");
}

#[test]
fn mismatched_integer_bound_types_reject_e2009() {
    rejects_with_e2009("fn main() { var n u32 = 5\nfor i in 0..n { _ = i } }");
    rejects_with_e2009("fn main() { var n i64 = 5\nfor i in 0..n { _ = i } }");
    rejects_with_e2009("fn main() { var lo i32 = 0\nvar hi i64 = 5\nfor i in lo..hi { _ = i } }");
    rejects_with_e2009("fn main() { var lo u8 = 0\nvar hi u16 = 5\nfor i in lo..hi { _ = i } }");
    rejects_with_e2009("fn main() { var lo i64 = 0\nfor i in lo..=10 { _ = i } }");
}

#[test]
fn overflowing_constant_range_bounds_reject_e2009() {
    rejects_with_e2009("fn main() { for i in 0..(1 << 63) { _ = i } }");
    rejects_with_e2009("fn main() { for i in 0..99999999999999999999 { _ = i } }");
    rejects_with_e2009("fn main() { var lo i8 = 0\nfor i in lo..300 { _ = i } }");
    rejects_with_e2009("fn main() { var hi u8 = 10\nfor i in -1..hi { _ = i } }");
}

#[test]
fn matching_integer_bounds_and_shadowed_vars_stay_accepted() {
    accepts_without_e2009("fn main() { var lo i64 = 0\nvar hi i64 = 5\nfor i in lo..hi { _ = i } }");
    accepts_without_e2009("fn main() { var lo i64 = 0\nfor i in lo..10 { _ = i } }");
    accepts_without_e2009("fn main() { var n u32 = 5\nfor i in u32(0)..n { _ = i } }");
    accepts_without_e2009("const f = 2.5\nfn main() { f := 5\nfor i in 0..f { _ = i } }");
}
