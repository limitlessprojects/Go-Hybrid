//! return_error_slot.rs — tests for Go-Hybrid #982 ((T, error) return signatures).

use hy_check::check_file;
use hy_syntax::parse_file;

fn rejects_with_e1002(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e1002 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.severity == hy_diag::Severity::Error);
    assert!(
        has_e1002,
        "expected HYB-E1002 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

fn accepts_without_e1002(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e1002 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.severity == hy_diag::Severity::Error);
    assert!(
        !has_e1002,
        "expected NO HYB-E1002 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

#[test]
fn single_error_slot_rejects_concrete_primitive_issue_982() {
    rejects_with_e1002("fn bad_single() error {\n    return 42\n}\n");
}

#[test]
fn pair_error_slot_rejects_concrete_primitive_issue_982() {
    rejects_with_e1002("fn bad_pair() (int, error) {\n    return 1, 2\n}\n");
}

#[test]
fn multi_value_call_return_non_error_tail_rejects_issue_982() {
    rejects_with_e1002(
        "fn pair() (int, int) {\n    return 1, 2\n}\n\
         fn worker() (int, error) {\n    return pair()\n}\n"
    );
}

#[test]
fn valid_error_slot_returns_accepted_issue_982() {
    accepts_without_e1002("fn pair() (int, error) {\n    return 1, nil\n}\n");
}
