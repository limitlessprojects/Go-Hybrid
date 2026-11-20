//! nonvariadic_spread_soundness_4013_4018.rs: regression tests for #4013 and #4018.
//!
//! Under Go specification (§Calls / §Conversions) and GUARANTEES.md Row 13 (BUILDABLE):
//! - `append()` is the ONLY built-in function that permits variadic spread `...`.
//! - All other built-ins (`panic`, `print`, `println`, `make`, `new`, `recover`, `some`, etc.)
//!   reject variadic spread `...` with `invalid use of ... with built-in <name>`.
//! - Type conversions `T(x)` require exactly one expression without variadic spread `...`.
//!
//! Verify that variadic spread `...` on non-variadic built-ins and type conversions
//! is rejected with HYB-E1002, while valid built-in calls, append spread, and normal
//! conversions remain accepted.

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

// ---------------------------------------------------------------- reject built-in spread (#4013):

#[test]
fn panic_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    msg := "boom"
    panic(msg...)
}
"#,
    );
}

#[test]
fn print_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []string{"a", "b"}
    print(xs...)
}
"#,
    );
}

#[test]
fn println_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []string{"a", "b"}
    println(xs...)
}
"#,
    );
}

#[test]
fn make_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    ns := []int{5}
    _ = make([]int, ns...)
}
"#,
    );
}

#[test]
fn new_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []int{1}
    _ = new(int...)
}
"#,
    );
}

#[test]
fn recover_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []int{1}
    _ = recover(xs...)
}
"#,
    );
}

#[test]
fn some_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []int{1}
    _ = some(xs...)
}
"#,
    );
}

// ---------------------------------------------------------------- reject type conversion spread (#4018):

#[test]
fn int_conversion_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []int{1, 2}
    _ = int(xs...)
}
"#,
    );
}

#[test]
fn string_conversion_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    bs := []byte{65, 66}
    _ = string(bs...)
}
"#,
    );
}

#[test]
fn float64_conversion_with_spread_rejects() {
    rejects(
        r#"
fn main() {
    xs := []float64{1.5}
    _ = float64(xs...)
}
"#,
    );
}

#[test]
fn named_type_conversion_with_spread_rejects() {
    rejects(
        r#"
type my_int = int

fn main() {
    xs := []int{10}
    _ = my_int(xs...)
}
"#,
    );
}

// ---------------------------------------------------------------- positive controls (valid usage):

#[test]
fn append_with_spread_accepted() {
    accepts(
        r#"
fn main() {
    s := []int{1, 2}
    tail := []int{3, 4}
    _ = append(s, tail...)
}
"#,
    );
}

#[test]
fn valid_builtins_without_spread_accepted() {
    accepts(
        r#"
fn main() {
    _ = make([]int, 5)
    _ = new(int)
    _ = some(42)
    _ = recover()
    print("msg")
    println("line")
}
"#,
    );
}

#[test]
fn valid_type_conversions_without_spread_accepted() {
    accepts(
        r#"
type my_int = int

fn main() {
    _ = int(42)
    _ = float64(10)
    _ = my_int(5)
    bs := []byte{65}
    _ = string(bs)
}
"#,
    );
}
