//! issue_2825_copy_slice_element_soundness.rs — tests for Issue #2825 (Row 13 BUILDABLE).
//!
//! Under Go §Built-in functions (copy):
//! - Both arguments must have identical element types, or
//! - As a special case, `src` can have string type if `dst` has type `[]byte`.
//!
//! Passing slices with mismatched concrete numeric element types (e.g. `[]int64` vs `[]int`,
//! `[]float64` vs `[]float32`) or copying a `string` into a non-byte slice (e.g. `[]int`,
//! `[]int64`, `[]rune`) must reject with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn copy_mismatched_slice_elements_rejected_e1002() {
    let cases = [
        (
            "int64_and_int_slices",
            r#"
fn main() {
    var dst []int64 = []int64{1, 2}
    var src []int = []int{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "int_and_int32_slices",
            r#"
fn main() {
    var dst []int = []int{1, 2}
    var src []int32 = []int32{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "float64_and_float32_slices",
            r#"
fn main() {
    var dst []float64 = []float64{1.0, 2.0}
    var src []float32 = []float32{3.0, 4.0}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "float32_and_float64_slices",
            r#"
fn main() {
    var dst []float32 = []float32{1.0, 2.0}
    var src []float64 = []float64{3.0, 4.0}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "inferred_slice_lit_mismatch",
            r#"
fn main() {
    dst := []int64{1, 2}
    src := []int{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "struct_field_slice_mismatch",
            r#"
struct holder {
    dst []int64
}

fn main() {
    var h holder
    var src []int = []int{1, 2}
    _ = copy(h.dst, src)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should be rejected, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn copy_string_to_non_byte_slice_rejected_e1002() {
    let cases = [
        (
            "string_to_int_slice",
            r#"
fn main() {
    var dst []int = []int{1, 2}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
        (
            "string_to_int64_slice",
            r#"
fn main() {
    var dst []int64 = []int64{1, 2}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
        (
            "string_to_rune_slice",
            r#"
fn main() {
    var dst []rune = []rune{1, 2}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
        (
            "string_to_float64_slice",
            r#"
fn main() {
    var dst []float64 = []float64{1.0, 2.0}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should be rejected, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn compatible_copy_cases_accepted() {
    let cases = [
        (
            "matching_int_slices",
            r#"
fn main() {
    var dst []int = []int{1, 2}
    var src []int = []int{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "matching_int64_slices",
            r#"
fn main() {
    var dst []int64 = []int64{1, 2}
    var src []int64 = []int64{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "matching_float64_slices",
            r#"
fn main() {
    var dst []float64 = []float64{1.0, 2.0}
    var src []float64 = []float64{3.0, 4.0}
    _ = copy(dst, src)
}
"#,
        ),
        (
            "string_to_byte_slice",
            r#"
fn main() {
    var dst []byte = []byte{1, 2}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
        (
            "string_to_uint8_slice",
            r#"
fn main() {
    var dst []uint8 = []uint8{1, 2}
    var s string = "hello"
    _ = copy(dst, s)
}
"#,
        ),
        (
            "byte_and_uint8_slice_alias_copy",
            r#"
fn main() {
    var dst []byte = []byte{1, 2}
    var src []uint8 = []uint8{3, 4}
    _ = copy(dst, src)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {name} expected Accept, got Reject with diagnostics: {:?}",
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "case {name} expected 0 errors, got {:?}",
            r.diagnostics
        );
    }
}
