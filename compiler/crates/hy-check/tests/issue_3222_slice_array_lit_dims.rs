//! issue_3222_slice_array_lit_dims.rs — issue #3222 (Row 13 BUILDABLE).
//!
//! Fixed-size array elements in slice literals `[][N]T{...}` and array literals
//! `[M][N]T{...}` must be validated against the declared array length via
//! `sized_array_names_differ`. Mismatches must be rejected at compile time
//! with HYB-E1002.

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
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn slice_lit_mismatched_array_len_var_rejects() {
    let r = check(
        r#"
fn main() {
    arr3 := [3]int{1, 2, 3}
    _ = [][2]int{arr3}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_lit_mismatched_array_len_literal_rejects() {
    let r = check(
        r#"
fn main() {
    _ = [][2]int{[3]int{1, 2, 3}}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn array_lit_mismatched_array_len_var_rejects() {
    let r = check(
        r#"
fn main() {
    arr3 := [3]int{1, 2, 3}
    _ = [2][2]int{arr3, arr3}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn array_lit_mismatched_array_len_literal_rejects() {
    let r = check(
        r#"
fn main() {
    _ = [2][2]int{[3]int{1, 2, 3}, [3]int{1, 2, 3}}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn var_decl_array_len_mismatches_reject() {
    let r1 = check(
        r#"
fn main() {
    var arr4 [4]int
    _ = [][2]int{arr4}
}
"#,
    );
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(
        error_codes(&r1).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r1.diagnostics
    );

    let r2 = check(
        r#"
fn main() {
    var arr4 [4]int
    _ = [1][2]int{arr4}
}
"#,
    );
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(
        error_codes(&r2).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r2.diagnostics
    );
}

#[test]
fn move_operand_array_len_mismatch_rejects() {
    let r = check(
        r#"
fn main() {
    arr3 := [3]int{1, 2, 3}
    _ = [][2]int{move arr3}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn matching_array_len_stays_accepted() {
    let r = check(
        r#"
fn main() {
    arr2 := [2]int{1, 2}
    _ = [][2]int{arr2}
    _ = [][2]int{[2]int{3, 4}}
    _ = [2][2]int{arr2, [2]int{5, 6}}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

#[test]
fn unkeyed_sub_composites_stay_accepted() {
    let r = check(
        r#"
fn main() {
    _ = [][]int{{1, 2}, {3, 4}}
    _ = [2][2]int{{1, 2}, {3, 4}}
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}
