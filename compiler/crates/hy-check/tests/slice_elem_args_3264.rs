//! Regression pin for #3264: slice element scalar widths in call arguments
//! and return statements.
//!
//! `OpKind` collapses every integer width, so `[]int32` and `[]int64` both
//! looked like `OpKind::Slice(Int)`; `scalar_of_ty` returns None for a slice
//! and `sized_array_names_differ` only handles sized arrays. Mismatched slice
//! arguments and returns therefore passed `hyb check` and failed `go build`.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("slice_elem_3264.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// The reported argument shape.
#[test]
fn slice_argument_with_mismatched_element_width_is_rejected() {
    let got = error_codes(
        r#"fn take_slice(s []int64) {
    _ = s
}

fn main() {
    var b []int32
    take_slice(b)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// The reported return shape.
#[test]
fn slice_return_with_mismatched_element_width_is_rejected() {
    let got = error_codes(
        r#"fn return_slice() []int64 {
    var b []int32
    return b
}

fn main() {
    _ = return_slice()
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Negative control: matching element widths stay accepted.
#[test]
fn matching_slice_element_widths_stay_accepted() {
    let got = error_codes(
        r#"fn take_slice(s []int64) {
    _ = s
}

fn ret_ok() []int64 {
    var b []int64
    return b
}

fn main() {
    var a []int64
    take_slice(a)
    _ = ret_ok()
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: `byte` and `uint8` are the same Go type and must not
/// conflict.
#[test]
fn byte_and_uint8_do_not_conflict() {
    let got = error_codes(
        r#"fn take_bytes(b []byte) {
    _ = b
}

fn main() {
    var u []uint8
    take_bytes(u)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: a slice whose element type the checker cannot resolve
/// must fail open rather than reject. A named slice type alias is the shape
/// that leaves the spelling unresolved.
#[test]
fn unresolved_slice_elements_stay_accepted() {
    let got = error_codes(
        r#"type ints = []int64

fn take_slice(s []int64) {
    _ = s
}

fn main() {
    var xs ints
    take_slice(xs)
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}