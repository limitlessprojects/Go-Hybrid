//! Regression test for issue #3261:
//! type-check: enforce scalar width soundness on index/field places and composite assignments.
//!
//! Under Go Spec and GUARANTEES.md Row 13 (BUILDABLE), assignments must enforce type equality:
//! 1. Index places (s[i] = x, m[k] = x, arr[i] = x, matrix[0][0] = x)
//! 2. Struct field places (f.val = x, pf.val = x, f.arr = other)
//! 3. Composite slice and array assignments (a = b)
//! 4. Nested slices (matrix[0] = row) and array dimensions/element scalars
//! 5. Parallel assignments (s[0], f.val = x, y)
//! All mismatches must reject with HYB-E1002.

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

#[test]
fn test_slice_index_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_slice_index() {
    var s []int64
    var x int32 = 1
    s[0] = x
    _ = s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for slice index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_map_index() {
    var m map[string]int64
    var x int32 = 1
    m["k"] = x
    _ = m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_array_index_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_array_index() {
    var arr [3]int64
    var x int32 = 1
    arr[0] = x
    _ = arr
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for array index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_struct_field_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
struct foo {
    val int64
}

fn test_field() {
    var f foo
    var x int32 = 1
    f.val = x
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for struct field scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_pointer_struct_field_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
struct foo {
    val int64
}

fn test_ptr_field(pf *foo) {
    var x int32 = 1
    pf.val = x
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for pointer struct field scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_struct_field_array_length_mismatch_rejects_e1002() {
    let src = r#"
struct foo {
    arr [3]int
}

fn test_field_arr() {
    var f foo
    var other [4]int
    f.arr = other
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for struct field array length mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_slice_variable_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_slice_assign() {
    var a []int64
    var b []int32
    a = b
    _ = a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for slice assignment scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_array_variable_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_array_assign() {
    var a [3]int64
    var b [3]int32
    a = b
    _ = a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for array assignment scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_nested_slice_index_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
fn test_nested_slice() {
    var matrix [][]int64
    var row []int32
    matrix[0] = row
    _ = matrix
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for nested slice assign scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_alias_type_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
type my_int = int64

fn test_alias() {
    var s []my_int
    var x int32 = 1
    s[0] = x
    _ = s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for type alias index scalar mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_parallel_assign_scalar_mismatch_rejects_e1002() {
    let src = r#"
struct foo {
    val int64
}

fn test_parallel() {
    var s []int64
    var f foo
    var x int32 = 1
    var y int64 = 2
    s[0], f.val = x, y
    _ = s
    _ = f
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for parallel assignment mismatch, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_matching_assignments_accepted() {
    let src = r#"
struct foo {
    val int64
    arr [3]int
}

fn test_valid() {
    var s []int64
    var x int64 = 1
    s[0] = x

    var m map[string]int64
    m["k"] = x

    var f foo
    f.val = x

    var other [3]int
    f.arr = other

    var a []int64
    var b []int64
    a = b

    _ = s
    _ = m
    _ = f
    _ = a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for matching assignments, got {:?}", r.diagnostics);
}
