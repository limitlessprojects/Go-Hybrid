//! issue_3255_composite_lit_array_scalar_soundness.rs
//!
//! Regression test for issue #3255:
//! Composite literals (struct, map, array, slice) must reject sized-array length
//! mismatches and element scalar width mismatches under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("parse error");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ==================== STRUCT LITERALS ====================

#[test]
fn struct_literal_array_length_mismatch_variable_rejects_e1002() {
    let src = "\
struct container {
    items [3]int
}

fn main() {
    var big [4]int
    c := container{items: big}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_array_length_mismatch_direct_literal_rejects_e1002() {
    let src = "\
struct container {
    items [3]int
}

fn main() {
    c := container{items: [4]int{1, 2, 3, 4}}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal length mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_array_scalar_width_mismatch_variable_rejects_e1002() {
    let src = "\
struct container {
    items [3]int32
}

fn main() {
    var big [3]int64
    c := container{items: big}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array scalar width mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_array_scalar_width_mismatch_direct_literal_rejects_e1002() {
    let src = "\
struct container {
    items [3]int32
}

fn main() {
    c := container{items: [3]int64{1, 2, 3}}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal scalar width mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_slice_scalar_width_mismatch_variable_rejects_e1002() {
    let src = "\
struct container {
    items []int32
}

fn main() {
    var big []int64
    c := container{items: big}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice scalar width mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_slice_scalar_width_mismatch_direct_literal_rejects_e1002() {
    let src = "\
struct container {
    items []int32
}

fn main() {
    c := container{items: []int64{1, 2}}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct slice literal scalar width mismatch in struct literal, got: {codes:?}"
    );
}

#[test]
fn struct_literal_aliased_array_length_mismatch_rejects_e1002() {
    let src = "\
type arr3 = [3]int
type arr4 = [4]int

struct container {
    items arr3
}

fn main() {
    var big arr4
    c := container{items: big}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for aliased array length mismatch, got: {codes:?}"
    );
}

#[test]
fn struct_literal_fn_call_result_array_length_mismatch_rejects_e1002() {
    let src = "\
struct container {
    items [3]int
}

fn make4() [4]int {
    var res [4]int
    return res
}

fn main() {
    c := container{items: make4()}
    _ = c
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for fn call array length mismatch in struct literal, got: {codes:?}"
    );
}

// ==================== MAP LITERALS ====================

#[test]
fn map_literal_value_sized_array_length_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var v [4]int
    m := map[string][3]int{\"a\": v}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch in map literal value, got: {codes:?}"
    );
}

#[test]
fn map_literal_value_sized_array_length_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    m := map[string][3]int{\"a\": [4]int{1, 2, 3, 4}}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal in map literal value, got: {codes:?}"
    );
}

#[test]
fn map_literal_key_sized_array_length_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var k [4]int
    m := map[[3]int]string{k: \"a\"}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch in map literal key, got: {codes:?}"
    );
}

#[test]
fn map_literal_key_sized_array_length_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    m := map[[3]int]string{[4]int{1, 2, 3, 4}: \"a\"}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal in map literal key, got: {codes:?}"
    );
}

#[test]
fn map_literal_value_slice_scalar_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var s []int64
    m := map[string][]int32{\"a\": s}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice scalar mismatch in map literal value, got: {codes:?}"
    );
}

#[test]
fn map_literal_value_slice_scalar_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    m := map[string][]int32{\"a\": []int64{1, 2}}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct slice literal scalar mismatch in map value, got: {codes:?}"
    );
}

// ==================== SLICE LITERALS ====================

#[test]
fn slice_literal_array_length_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var v [4]int
    s := [][3]int{v}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch in slice literal, got: {codes:?}"
    );
}

#[test]
fn slice_literal_array_length_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    s := [][3]int{[4]int{1, 2, 3, 4}}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal in slice literal, got: {codes:?}"
    );
}

#[test]
fn slice_literal_nested_slice_scalar_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var sub []int64
    s := [][]int32{sub}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for nested slice scalar mismatch in slice literal, got: {codes:?}"
    );
}

#[test]
fn slice_literal_nested_slice_scalar_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    s := [][]int32{[]int64{1, 2}}
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct nested slice literal scalar mismatch in slice literal, got: {codes:?}"
    );
}

// ==================== ARRAY LITERALS ====================

#[test]
fn array_literal_array_length_mismatch_variable_rejects_e1002() {
    let src = "\
fn main() {
    var v [4]int
    a := [2][3]int{v}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array length mismatch in array literal, got: {codes:?}"
    );
}

#[test]
fn array_literal_array_length_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    a := [2][3]int{[4]int{1, 2, 3, 4}}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct array literal in array literal, got: {codes:?}"
    );
}

#[test]
fn array_literal_nested_slice_scalar_mismatch_direct_literal_rejects_e1002() {
    let src = "\
fn main() {
    a := [2][]int32{[]int64{1, 2}}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for direct nested slice literal scalar mismatch in array literal, got: {codes:?}"
    );
}

// ==================== CONTROLS (MUST ACCEPT) ====================

#[test]
fn matching_composite_literals_accept() {
    let src = "\
struct container {
    items [3]int
    names []string
}

fn main() {
    var exact [3]int
    var n []string
    c := container{items: exact, names: n}
    _ = c

    m := map[string][3]int{\"ok\": exact}
    _ = m

    s := [][3]int{exact}
    _ = s

    a := [1][3]int{exact}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected accept for matching composite literals, got: {codes:?}"
    );
}

#[test]
fn matching_direct_literals_accept() {
    let src = "\
struct container {
    items [3]int
}

fn main() {
    c := container{items: [3]int{1, 2, 3}}
    _ = c

    m := map[string][3]int{\"ok\": [3]int{1, 2, 3}}
    _ = m

    s := [][3]int{[3]int{1, 2, 3}}
    _ = s

    a := [1][3]int{[3]int{1, 2, 3}}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected accept for matching direct literals, got: {codes:?}"
    );
}
