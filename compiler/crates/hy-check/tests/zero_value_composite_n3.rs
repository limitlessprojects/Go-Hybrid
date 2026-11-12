//! Regression tests for #2466: struct composite literals and fixed-size array literals
//! that elide fields or elements of types without zero values must be rejected
//! with HYB-E4002 under SPEC-LANGUAGE §3.1 N3.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_e4002(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected rejection with HYB-E4002, but accepted: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4002"),
        "expected HYB-E4002 diagnostic code, got: {:?}",
        res.diagnostics
    );
}

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected acceptance, but rejected with: {:?}",
        res.diagnostics
    );
}

#[test]
fn struct_lit_missing_no_zero_value_field_rejects() {
    // Enum shape has no zero value because its first variant carries a payload (N3).
    let src = "enum shape {
    circle(f64)
    point
}
struct box {
    s shape
    count int
}
fn main() {
    b := box{count: 1}
    fmt.println(b)
}
";
    assert_reject_e4002(src);
}

#[test]
fn struct_lit_explicitly_initialized_accepts() {
    // When the non-zero-value field is explicitly supplied, the literal is valid.
    let src = "enum shape {
    circle(f64)
    point
}
struct box {
    s shape
    count int
}
fn main() {
    b := box{s: shape.point, count: 1}
    fmt.println(b)
}
";
    assert_accept(src);
}

#[test]
fn struct_lit_with_zero_value_enum_elided_field_accepts() {
    // Enum mode HAS a zero value because its first variant is payload-free.
    let src = "enum mode {
    read
    write(string)
}
struct config {
    m mode
    timeout int
}
fn main() {
    c := config{timeout: 30}
    fmt.println(c)
}
";
    assert_accept(src);
}

#[test]
fn array_lit_short_no_zero_value_element_rejects() {
    let src = "enum shape {
    circle(f64)
    point
}
fn main() {
    arr := [2]shape{shape.point}
    fmt.println(arr)
}
";
    assert_reject_e4002(src);
}

#[test]
fn array_lit_full_no_zero_value_element_accepts() {
    let src = "enum shape {
    circle(f64)
    point
}
fn main() {
    arr := [2]shape{shape.point, shape.circle(3.14)}
    fmt.println(arr)
}
";
    assert_accept(src);
}
