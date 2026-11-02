//! Regression test for Issue #3150:
//! copy() accepts mismatched slice element scalar widths and non-byte slices for string copy (violates Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_issue_3150_copy_element_scalar_width_mismatches_rejected() {
    let cases = [
        // Case 1: []int32 vs []int64
        (
            "copy_int32_int64",
            r#"
fn main() {
    dst := make([]int32, 5)
    src := make([]int64, 5)
    copy(dst, src)
}
"#,
            "int32",
            "int64",
        ),
        // Case 2: []int64 vs []int32
        (
            "copy_int64_int32",
            r#"
fn main() {
    dst := make([]int64, 5)
    src := make([]int32, 5)
    copy(dst, src)
}
"#,
            "int64",
            "int32",
        ),
        // Case 3: []float32 vs []float64
        (
            "copy_float32_float64",
            r#"
fn main() {
    dst := make([]float32, 5)
    src := make([]float64, 5)
    copy(dst, src)
}
"#,
            "float32",
            "float64",
        ),
        // Case 4: []int vs []int64
        (
            "copy_int_int64",
            r#"
fn main() {
    dst := make([]int, 5)
    src := make([]int64, 5)
    copy(dst, src)
}
"#,
            "int",
            "int64",
        ),
        // Case 5: array slice base with mismatched width: dst []int32, arr [4]int64
        (
            "copy_slice_from_mismatched_array",
            r#"
fn main() {
    dst := make([]int32, 4)
    arr := [4]int64{1, 2, 3, 4}
    copy(dst, arr[:])
}
"#,
            "int32",
            "int64",
        ),
        // Case 6: fn return slice with mismatched width
        (
            "copy_from_fn_return_mismatched",
            r#"
fn get_i64() []int64 {
    return make([]int64, 3)
}
fn main() {
    dst := make([]int32, 3)
    copy(dst, get_i64())
}
"#,
            "int32",
            "int64",
        ),
    ];

    for (name, code, d_ty, s_ty) in cases {
        let file = parse_file(&format!("{name}.hy"), code).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "case `{name}` must be rejected by hy-check (Go refuses mismatched copy elements)"
        );
        let diag = res
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E1002")
            .unwrap_or_else(|| panic!("case `{name}` expected HYB-E1002, got {:?}", res.diagnostics));
        assert!(
            diag.message.contains("different element types"),
            "case `{name}` message `{}` should mention 'different element types'",
            diag.message
        );
        assert!(
            diag.message.contains(d_ty) && diag.message.contains(s_ty),
            "case `{name}` message `{}` should mention `{d_ty}` and `{s_ty}`",
            diag.message
        );
    }
}

#[test]
fn test_issue_3150_copy_string_into_non_byte_slice_rejected() {
    let cases = [
        // Case 1: string into []int64
        (
            "copy_str_into_int64",
            r#"
fn main() {
    dst := make([]int64, 10)
    copy(dst, "hello")
}
"#,
            "int64",
        ),
        // Case 2: string into []int
        (
            "copy_str_into_int",
            r#"
fn main() {
    dst := make([]int, 10)
    copy(dst, "hello")
}
"#,
            "int",
        ),
        // Case 3: string into []int32
        (
            "copy_str_into_int32",
            r#"
fn main() {
    dst := make([]int32, 10)
    copy(dst, "hello")
}
"#,
            "int32",
        ),
    ];

    for (name, code, expected_ty) in cases {
        let file = parse_file(&format!("{name}.hy"), code).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "case `{name}` must be rejected (copying string requires []byte)"
        );
        let diag = res
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E1002")
            .unwrap_or_else(|| panic!("case `{name}` expected HYB-E1002, got {:?}", res.diagnostics));
        assert!(
            diag.message.contains("cannot copy string into slice"),
            "case `{name}` message `{}` should state 'cannot copy string into slice'",
            diag.message
        );
        assert!(
            diag.message.contains(expected_ty),
            "case `{name}` message `{}` should mention element type `{expected_ty}`",
            diag.message
        );
    }
}

#[test]
fn test_issue_3150_copy_matching_types_accepted() {
    let cases = [
        // Control 1: matching []int32
        (
            "copy_matching_int32",
            r#"
fn main() {
    dst := make([]int32, 5)
    src := make([]int32, 5)
    copy(dst, src)
}
"#,
        ),
        // Control 2: string into []byte
        (
            "copy_str_into_byte",
            r#"
fn main() {
    dst := make([]byte, 10)
    copy(dst, "hello")
}
"#,
        ),
        // Control 3: string into []uint8
        (
            "copy_str_into_uint8",
            r#"
fn main() {
    dst := make([]uint8, 10)
    copy(dst, "hello")
}
"#,
        ),
        // Control 4: array slice base matching dst
        (
            "copy_slice_matching_array",
            r#"
fn main() {
    dst := make([]int64, 4)
    arr := [4]int64{1, 2, 3, 4}
    copy(dst, arr[:])
}
"#,
        ),
        // Control 5: fn return slice matching dst
        (
            "copy_from_fn_return_matching",
            r#"
fn get_i32() []int32 {
    return make([]int32, 3)
}
fn main() {
    dst := make([]int32, 3)
    copy(dst, get_i32())
}
"#,
        ),
    ];

    for (name, code) in cases {
        let file = parse_file(&format!("{name}.hy"), code).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Accept,
            "case `{name}` with identical types must be accepted, got diagnostics: {:?}",
            res.diagnostics
        );
    }
}
