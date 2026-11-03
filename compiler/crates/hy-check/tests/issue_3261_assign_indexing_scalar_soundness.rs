//! issue_3261_assign_indexing_scalar_soundness.rs
//!
//! Regression test for issue #3261:
//! Assignments to indexed places (slices, maps, arrays), struct field places,
//! and composite slice variables must enforce scalar width soundness under HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
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
fn slice_index_assign_scalar_width_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    var s []int64
    var x int32 = 1
    s[0] = x
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice index scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn map_index_assign_scalar_width_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    var m map[string]int64
    var x int32 = 1
    m[\"k\"] = x
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for map index scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn array_index_assign_scalar_width_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    var a [3]int64
    var x int32 = 1
    a[0] = x
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for array index scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn struct_field_assign_scalar_width_mismatch_rejects_e1002() {
    let src = "\
struct foo {
    val int64
}

fn main() {
    var f foo
    var x int32 = 1
    f.val = x
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for struct field scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn slice_variable_assign_element_scalar_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    var a []int64
    var b []int32
    a = b
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for slice variable element scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn aliased_slice_variable_assign_scalar_mismatch_rejects_e1002() {
    let src = "\
type my_i64s = []int64
type my_i32s = []int32

fn main() {
    var a my_i64s
    var b my_i32s
    a = b
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for aliased slice variable scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn parallel_assign_slice_scalar_mismatch_rejects_e1002() {
    let src = "\
fn main() {
    var a []int64
    var b []int32
    var x int
    var y int
    a, x = b, y
    _ = a
    _ = x
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for parallel assignment slice scalar mismatch, got: {codes:?}"
    );
}

#[test]
fn matching_assignments_accept() {
    let src = "\
struct foo {
    val int64
}

fn main() {
    var s []int64
    var x64 int64 = 10
    s[0] = x64
    _ = s

    var m map[string]int64
    m[\"ok\"] = x64
    _ = m

    var a [3]int64
    a[0] = x64
    _ = a

    var f foo
    f.val = x64
    _ = f

    var s2 []int64
    s2 = s
    _ = s2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected accept for matching assignments, got: {codes:?}"
    );
}
