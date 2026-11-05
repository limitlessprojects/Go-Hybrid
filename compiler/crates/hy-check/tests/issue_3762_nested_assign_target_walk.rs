//! issue_3762_nested_assign_target_walk.rs — regression tests for Issue #3762:
//! hy-check/walk: collect calls and fnlits across nested Stmt::Assign target places (HYB-E3102).

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
fn field_assign_target_index_call_mutating_map_with_live_slot_loan_rejects_e3102() {
    let src = r#"
struct container {
    val int
}
fn mutate(m map[string]int) int {
    m["b"] = 2
    return 0
}
fn bad() {
    m := map[string]int{"a": 1}
    slot := &m["a"]
    var arr [1]container
    arr[mutate(m)].val = 99
    println(slot)
}
fn main() {
    bad()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn nested_index_assign_target_outer_index_call_rejects_e3102() {
    let src = r#"
fn mutate(m map[string]int) int {
    m["b"] = 2
    return 0
}
fn bad_matrix() {
    m := map[string]int{"a": 1}
    slot := &m["a"]
    var mat [1][1]int
    mat[mutate(m)][0] = 99
    println(slot)
}
fn main() {
    bad_matrix()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102, got {:?}",
        error_codes(&r)
    );
}

#[test]
fn non_mutating_index_call_preserves_valid_execution() {
    let src = r#"
struct container {
    val int
}
fn compute() int {
    return 0
}
fn good() {
    var arr [1]container
    arr[compute()].val = 99
}
fn main() {
    good()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}
