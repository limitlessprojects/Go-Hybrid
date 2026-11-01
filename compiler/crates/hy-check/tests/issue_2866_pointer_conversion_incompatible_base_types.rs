//! Regression test suite for Issue #2866 (Row 13 BUILDABLE Soundness track).
//!
//! Under Go language specification (§Conversions):
//! "A non-constant value x can be converted to type T in any of these cases:
//!  - x's type and T have identical underlying types.
//!  - x's type and T are unnamed pointer types and their pointer base types have identical underlying types.
//!  - x's type or T is an unsafe.Pointer."
//!
//! Converting pointers between distinct scalar widths (*int32 to *int64, *float32 to *float64),
//! signedness (*int to *uint), distinct structs (*point to *node), or container shapes (*[]int32 to *[]int64)
//! is illegal in Go and rejected with HYB-E1002.

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 among diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn pointer_conversion_mismatched_integer_widths_rejected() {
    // *int32 to *int64
    let i32_to_i64 = r#"
type ptr_a = *int32
type ptr_b = *int64

fn main() {
    var a ptr_a
    var b ptr_b = ptr_b(a)
    _ = b
}
"#;
    assert_e1002(i32_to_i64);

    // *int to *int64
    let int_to_i64 = r#"
type ptr_i64 = *int64

fn main() {
    var x int
    px := &x
    p := ptr_i64(px)
    _ = p
}
"#;
    assert_e1002(int_to_i64);

    // *int to *uint (signedness mismatch)
    let int_to_uint = r#"
type ptr_uint = *uint

fn main() {
    var x int
    px := &x
    p := ptr_uint(px)
    _ = p
}
"#;
    assert_e1002(int_to_uint);
}

#[test]
fn pointer_conversion_mismatched_float_widths_rejected() {
    let f32_to_f64 = r#"
type ptr_f32 = *float32
type ptr_f64 = *float64

fn main() {
    var a ptr_f32
    var b ptr_f64 = ptr_f64(a)
    _ = b
}
"#;
    assert_e1002(f32_to_f64);
}

#[test]
fn pointer_conversion_distinct_structs_rejected() {
    let structs = r#"
struct point {
    x int64
}

struct node {
    val int64
}

type ptr_point = *point
type ptr_node = *node

fn main() {
    var p ptr_point
    var n ptr_node = ptr_node(p)
    _ = n
}
"#;
    assert_e1002(structs);
}

#[test]
fn pointer_conversion_mismatched_containers_rejected() {
    let slices = r#"
type ptr_slice_i32 = *[]int32
type ptr_slice_i64 = *[]int64

fn main() {
    var a ptr_slice_i32
    var b ptr_slice_i64 = ptr_slice_i64(a)
    _ = b
}
"#;
    assert_e1002(slices);

    let arrays = r#"
type ptr_arr_4 = *[4]int
type ptr_arr_8 = *[8]int

fn main() {
    var a ptr_arr_4
    var b ptr_arr_8 = ptr_arr_8(a)
    _ = b
}
"#;
    assert_e1002(arrays);
}

#[test]
fn pointer_conversion_identical_underlying_types_accepted() {
    let valid = r#"
type my_int = int
type ptr_int = *int
type ptr_my_int = *my_int

struct point {
    x int64
}

type ptr_point = *point

fn main() {
    var a ptr_int
    var b ptr_my_int = ptr_my_int(a)

    var p ptr_point
    var p2 ptr_point = ptr_point(p)

    _ = b
    _ = p2
}
"#;
    assert_ok(valid);
}
