//! Regression test for Issue #3157:
//! place_scalar ignores indexing and field places, allowing mismatched scalar widths in assignments and compound assignments.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_issue_3157_place_scalar_assignment_mismatches_rejected() {
    let cases = [
        // Case 1: Struct field plain assignment width mismatch
        (
            "struct_field_plain_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    var val int64 = 100
    p.x = val
}
"#,
        ),
        // Case 2: Slice indexing plain assignment width mismatch
        (
            "slice_index_plain_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int64 = 100
    s[0] = val
}
"#,
        ),
        // Case 3: Array indexing plain assignment width mismatch
        (
            "array_index_plain_assign",
            r#"
fn main() {
    var arr [4]int32
    var val int64 = 100
    arr[0] = val
}
"#,
        ),
        // Case 4: Struct field to struct field width mismatch
        (
            "field_to_field_assign",
            r#"
struct box_a {
    val int32
}
struct box_b {
    val int64
}
fn main() {
    a := box_a{val: 10}
    b := box_b{val: 20}
    a.val = b.val
}
"#,
        ),
        // Case 5: Slice index to array index width mismatch
        (
            "slice_to_array_assign",
            r#"
fn main() {
    s := make([]int32, 4)
    arr := [4]int64{1, 2, 3, 4}
    s[0] = arr[1]
}
"#,
        ),
    ];

    for (name, code) in cases {
        let file = parse_file(&format!("{name}.hy"), code).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "case `{name}` must be rejected by hy-check (Go refuses mismatched assignment)"
        );
        let diag = res
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E1002")
            .unwrap_or_else(|| panic!("case `{name}` expected HYB-E1002, got {:?}", res.diagnostics));
        assert!(
            diag.message.contains("cannot assign a value of type") || diag.message.contains("cannot use"),
            "case `{name}` message `{}` should mention assign/use error",
            diag.message
        );
    }
}

#[test]
fn test_issue_3157_place_scalar_compound_assignments_rejected() {
    let cases = [
        // Case 1: Slice index += mismatch
        (
            "slice_index_add_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int64 = 100
    s[0] += val
}
"#,
        ),
        // Case 2: Slice index -= mismatch
        (
            "slice_index_sub_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int64 = 100
    s[0] -= val
}
"#,
        ),
        // Case 3: Slice index *= mismatch
        (
            "slice_index_mul_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int64 = 100
    s[0] *= val
}
"#,
        ),
        // Case 4: Slice index %= mismatch
        (
            "slice_index_mod_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int64 = 100
    s[0] %= val
}
"#,
        ),
        // Case 5: Struct field += mismatch
        (
            "struct_field_add_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    var val int64 = 100
    p.x += val
}
"#,
        ),
        // Case 6: Struct field *= mismatch
        (
            "struct_field_mul_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    var val int64 = 100
    p.x *= val
}
"#,
        ),
    ];

    for (name, code) in cases {
        let file = parse_file(&format!("{name}.hy"), code).unwrap();
        let res = check_file(&file);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "case `{name}` must be rejected (Go refuses compound assignment with mismatched types)"
        );
        let diag = res
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E1002")
            .unwrap_or_else(|| panic!("case `{name}` expected HYB-E1002, got {:?}", res.diagnostics));
        assert!(
            diag.message.contains("invalid operation")
                || diag.message.contains("invalid compound assignment")
                || diag.message.contains("invalid integer compound assignment"),
            "case `{name}` message `{}` should mention invalid operation/compound assignment",
            diag.message
        );
    }
}

#[test]
fn test_issue_3157_place_scalar_matching_controls_accepted() {
    let cases = [
        // Control 1: Struct field matching assignment
        (
            "struct_field_matching_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    var val int32 = 100
    p.x = val
}
"#,
        ),
        // Control 2: Slice indexing matching assignment
        (
            "slice_index_matching_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int32 = 100
    s[0] = val
}
"#,
        ),
        // Control 3: Slice indexing matching compound assignment
        (
            "slice_index_matching_add_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    var val int32 = 100
    s[0] += val
}
"#,
        ),
        // Control 4: Struct field matching compound assignment
        (
            "struct_field_matching_add_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    var val int32 = 100
    p.x += val
}
"#,
        ),
        // Control 5: Untyped constant literal to slice element
        (
            "slice_index_literal_assign",
            r#"
fn main() {
    s := make([]int32, 5)
    s[0] = 42
    s[0] += 1
}
"#,
        ),
        // Control 6: Untyped constant literal to struct field
        (
            "struct_field_literal_assign",
            r#"
struct point {
    x int32
}
fn main() {
    p := point{x: 1}
    p.x = 42
    p.x += 1
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
            "case `{name}` must be accepted, got diagnostics: {:?}",
            res.diagnostics
        );
    }
}
