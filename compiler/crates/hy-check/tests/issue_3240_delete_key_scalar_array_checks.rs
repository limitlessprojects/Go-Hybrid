//! Regression pin for #3240:
//! Type Soundness: Enforce Scalar Numeric Width and Sized Array Matching for Built-in delete Key Arguments.
//!
//! SPEC-LANGUAGE §3.1 and GUARANTEES.md Row 13 (BUILDABLE):
//! Any code accepted by `hyb check` must build cleanly with `go build`.
//!
//! Built-in `delete(m, k)` on `map[K]V` and `delete(s, v)` on `set[T]` must reject:
//! - Scalar numeric width mismatches (e.g. `k: int64` on `set[int32]`) with HYB-E2005.
//! - Sized array length mismatches (e.g. `k: [3]int` on `map[[2]int]V` or `set[[2]int]`) with HYB-E2005.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("issue_3240.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn map_delete_with_mismatched_sized_array_key_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var m map[[2]int]string
    var k [3]int
    delete(m, k)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for map[[2]int] with [3]int key, got {got:?}"
    );
}

#[test]
fn set_delete_with_mismatched_scalar_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var s set[int32]
    var k int64 = 1
    delete(s, k)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for set[int32] with int64 element, got {got:?}"
    );

    let got_uint = error_codes(
        r#"fn main() {
    var s set[uint8]
    var k int = 1
    delete(s, k)
}
"#,
    );
    assert!(
        got_uint.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for set[uint8] with int element, got {got_uint:?}"
    );
}

#[test]
fn set_delete_with_mismatched_sized_array_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var s set[[2]int]
    var k [3]int
    delete(s, k)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for set[[2]int] with [3]int element, got {got:?}"
    );
}

#[test]
fn matching_delete_arguments_are_accepted() {
    let got = error_codes(
        r#"fn main() {
    var m map[[2]int]string
    var k [2]int
    delete(m, k)

    var s set[int32]
    var k2 int32 = 1
    delete(s, k2)
    delete(s, 1)

    var s2 set[[2]int]
    var k3 [2]int
    delete(s2, k3)

    var s3 set[string]
    delete(s3, "hello")
}
"#,
    );
    assert!(got.is_empty(), "expected no errors for valid delete, got {got:?}");
}
