//! Regression pin for #3242: `copy` element-type checks must see through
//! `OpKind`'s collapsing.
//!
//! `OpKind::Int` covers every integer width and `OpKind::Array` covers every
//! fixed-size array, so a kind-only comparison accepted `copy([]int32,
//! []int64)` and `copy([][2]int, [][3]int)` with 0 diagnostics while `go
//! build` refused them. The string arm had the same shape: `OpKind::Int`
//! let `copy([]int, "s")` through, but Go requires a byte destination.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("copy_elem_3242.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// Slice-to-slice with different integer widths: `int32` vs `int64`.
#[test]
fn copy_between_different_integer_widths_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var dst []int32
    var src []int64
    n := copy(dst, src)
    fmt.println(n)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Slice-of-array with different fixed dimensions: `[2]int` vs `[3]int`.
#[test]
fn copy_between_different_array_lengths_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var dst [][2]int
    var src [][3]int
    n := copy(dst, src)
    fmt.println(n)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// String source into a non-byte destination.
#[test]
fn copy_from_string_into_int_slice_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var dst []int
    n := copy(dst, "hello")
    fmt.println(n)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Negative control: the legal shapes must stay accepted.
#[test]
fn legal_copy_shapes_stay_accepted() {
    let got = error_codes(
        r#"fn main() {
    var b []byte
    n1 := copy(b, "hello")
    var d []int
    var s []int
    n2 := copy(d, s)
    var da [][2]int
    var sa [][2]int
    n3 := copy(da, sa)
    var u []uint8
    n4 := copy(u, "hi")
    fmt.println(n1, n2, n3, n4)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: an element type the checker cannot resolve must fail
/// open, not reject.
#[test]
fn unresolved_element_types_stay_accepted() {
    let got = error_codes(
        r#"fn main() {
    xs := make([]int, 3)
    ys := make([]int, 3)
    fmt.println(copy(xs, ys))
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}