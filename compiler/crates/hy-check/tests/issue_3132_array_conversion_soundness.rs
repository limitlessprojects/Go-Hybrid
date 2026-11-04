//! Regression test for issue #3132:
//! [soundness] hy-check: array type conversions with mismatched lengths or element scalars pass check and fail go build (Row 13 BUILDABLE)
//!
//! Under Go specification (§Conversions) and Row 13 (BUILDABLE):
//! - Array-to-array conversion requires identical lengths and identical element types.
//! - Slice-to-array conversion requires identical element types.
//! Mismatches must be rejected with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet should parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// 1. Array-to-array length mismatch between named type aliases
#[test]
fn test_array_to_array_len_mismatch_named_types() {
    let src = r#"
package main
type a_arr = [3]int
type b_arr = [4]int

fn test_len(a a_arr) {
    _ = b_arr(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 2. Array-to-array element scalar mismatch between named type aliases
#[test]
fn test_array_to_array_scalar_mismatch_named_types() {
    let src = r#"
package main
type a_arr32 = [4]int32
type b_arr64 = [4]int64

fn test_scalar(a a_arr32) {
    _ = b_arr64(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 3. Slice-to-array element scalar mismatch between named type aliases
#[test]
fn test_slice_to_array_scalar_mismatch_named_types() {
    let src = r#"
package main
type s32 = []int32
type b_arr64 = [4]int64

fn test_slice(s s32) {
    _ = b_arr64(s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 4. Array-to-array literal length mismatch
#[test]
fn test_array_to_array_literal_len_mismatch() {
    let src = r#"
package main
fn main() {
    _ = [4]int{}([3]int{1, 2, 3})
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 5. Array-to-array literal element scalar mismatch
#[test]
fn test_array_to_array_literal_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    _ = [4]int64{}([4]int32{1, 2, 3, 4})
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 6. Slice variable to array literal scalar mismatch
#[test]
fn test_slice_to_array_literal_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    s := []int32{1, 2, 3, 4}
    _ = [4]int64{}(s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 7. Slice literal to array conversion scalar mismatch
#[test]
fn test_slice_literal_to_array_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    _ = [4]int64{}([]int32{1, 2, 3, 4})
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 8. Array variable to array literal length mismatch
#[test]
fn test_array_to_array_len_mismatch_with_local_binding() {
    let src = r#"
package main
fn main() {
    a := [3]int{1, 2, 3}
    _ = [4]int{}(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 9. Array variable to array literal element scalar mismatch
#[test]
fn test_array_to_array_scalar_mismatch_with_local_binding() {
    let src = r#"
package main
fn main() {
    a := [4]int32{1, 2, 3, 4}
    _ = [4]int64{}(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 10. Nested array dimension mismatch in conversion
#[test]
fn test_array_conversion_nested_dimension_mismatch() {
    let src = r#"
package main
type a_mat = [2][3]int
type b_mat = [2][4]int

fn test_mat(a a_mat) {
    _ = b_mat(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 11. Nested slice element scalar mismatch in array conversion
#[test]
fn test_array_conversion_nested_slice_scalar_mismatch() {
    let src = r#"
package main
type a_mat = [2][]int32
type b_mat = [2][]int64

fn test_mat(a a_mat) {
    _ = b_mat(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 12. Deep type alias resolution in array conversion
#[test]
fn test_array_conversion_type_alias_unrolling() {
    let src = r#"
package main
type my_i32 = int32
type my_i64 = int64
type a_arr = [4]my_i32
type b_arr = [4]my_i64

fn test_alias(a a_arr) {
    _ = b_arr(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 13. Valid matching array conversions must be accepted
#[test]
fn test_matching_array_conversions_accepted() {
    let src = r#"
package main
type a_arr = [3]int
type b_arr = [3]int
type my_int = int
type my_arr = [3]my_int

fn run(a a_arr, s []int) {
    _ = b_arr(a)
    _ = my_arr(a)
    _ = [3]int{}(s)
    _ = [3]int{}([3]int{1, 2, 3})
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert_eq!(error_codes(&r), Vec::<String>::new());
}
