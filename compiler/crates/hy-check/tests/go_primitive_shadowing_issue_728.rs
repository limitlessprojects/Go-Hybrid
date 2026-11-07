//! #728 — [soundness] A Hybrid identifier spelled like a Go type (`float64`, `int64`,
//! `uint8`, ...) captures the emitter's lowering of `f64(x)` / `i64(x)` / `u8(x)`:
//! accepted program builds and prints a wrong result (or fails go build).
//!
//! A user declaration (fn, const, local variable, parameter, struct, enum, type alias)
//! must not use Go primitive type names whose Hybrid spellings differ (`f64`, `i64`, `u8`, etc.)
//! or runtime-injected emitter names (`hy_pop`, `filepath`).
//!
//! Using them as TYPE names or conversion targets (e.g. `var x float64 = 1.5`, `float64(x)`)
//! remains valid syntax and must stay accepted.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == want),
        "expected [{want}] in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

// ---------------------------------------------------------------------------
// Rejections: user declarations named after Go primitive types or injected names
// ---------------------------------------------------------------------------

#[test]
fn float64_fn_shadowing_conversion_rejected() {
    // Lead repro from issue #728:
    // fn float64(x int) int captured f64(n) / 2 in emitted Go and printed 3500 instead of 3.5.
    let src = r#"
package main

fn float64(x int) int {
    return x * 1000
}

fn main() {
    n := 7
    half := f64(n) / 2
    fmt.println(half)
}
"#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn local_int64_shadowing_conversion_rejected() {
    // Second repro from issue #728:
    // int64 := 3 shadows i64(7) in emitted Go, failing go build:
    // `invalid operation: cannot call int64 (variable of type int): int is not a function`.
    let src = r#"
fn main() {
    int64 := 3
    x := i64(7)
    fmt.println(int64, x)
}
"#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn param_uint8_shadowing_conversion_rejected() {
    let src = r#"
fn f(uint8 int) int {
    return uint8
}
fn main() {
    x := f(5)
    fmt.println(x)
}
"#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn const_float32_rejected() {
    let src = r#"
const float32 = 1.5
fn main() {
    fmt.println(float32)
}
"#;
    assert_code(src, "HYB-E1002");
}

#[test]
fn emitter_injected_names_rejected_as_declarations() {
    let src_fp = r#"
fn filepath() int {
    return 1
}
fn main() {
    fmt.println(filepath())
}
"#;
    assert_code(src_fp, "HYB-E1002");

    let src_pop = r#"
fn hy_pop() int {
    return 1
}
fn main() {
    fmt.println(hy_pop())
}
"#;
    assert_code(src_pop, "HYB-E1002");
}

// ---------------------------------------------------------------------------
// Controls: using Go primitive type names as TYPES remains accepted
// ---------------------------------------------------------------------------

#[test]
fn go_primitive_type_names_in_type_positions_stay_accepted() {
    let src = r#"
fn scale(f float64) float64 {
    return f * 2.0
}

fn main() {
    var x float64 = 1.5
    var y int64 = 10
    var z uint8 = 20
    s := scale(x)
    fmt.println(x, y, z, s)
}
"#;
    assert_ok(src);
}

#[test]
fn standard_f64_conversions_stay_accepted() {
    let src = r#"
fn main() {
    n := 7
    half := f64(n) / 2
    fmt.println(half)
}
"#;
    assert_ok(src);
}

#[test]
fn struct_fields_named_like_primitive_stay_accepted() {
    let src = r#"
struct record {
    float64 int
    int64 string
}
fn main() {
    var r record
    r.float64 = 42
    r.int64 = "ok"
    fmt.println(r.float64, r.int64)
}
"#;
    assert_ok(src);
}
