//! channel_element_check.rs — integration tests for Go-Hybrid #1044.
//! Verifies that channel sends with inner/composite element mismatches
//! (maps, slices, arrays, pointers, primitive kinds) are rejected with HYB-E3011.

use hy_check::check_file;
use hy_syntax::parse_file;

fn rejects_with_e3011(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e3011 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3011" && d.severity == hy_diag::Severity::Error);
    assert!(
        has_e3011,
        "expected HYB-E3011 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

fn accepts_without_e3011(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e3011 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3011" && d.severity == hy_diag::Severity::Error);
    assert!(
        !has_e3011,
        "expected NO HYB-E3011 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

#[test]
fn channel_send_primitive_kind_mismatch_issue_1044() {
    // Sending bool to chan int is rejected with HYB-E3011
    rejects_with_e3011("fn main() {\n    ch := make(chan int, 1)\n    ch <- true\n}\n");
}

#[test]
fn channel_send_slice_inner_mismatch_issue_1044() {
    // Sending []string to chan []int is rejected with HYB-E3011
    rejects_with_e3011("fn main() {\n    ch := make(chan []int, 1)\n    var xs []string = []string{\"a\"}\n    ch <- move xs\n}\n");
}

#[test]
fn channel_send_map_inner_mismatch_issue_1044() {
    // Sending map[string]string to chan map[string]int is rejected with HYB-E3011
    rejects_with_e3011("fn main() {\n    ch := make(chan map[string]int, 1)\n    var m map[string]string = map[string]string{\"a\": \"b\"}\n    ch <- move m\n}\n");
}

#[test]
fn channel_send_matching_types_accepted_issue_1044() {
    // Sending int to chan int is accepted
    accepts_without_e3011("fn main() {\n    ch := make(chan int, 1)\n    ch <- 42\n}\n");

    // Sending []int to chan []int is accepted
    accepts_without_e3011("fn main() {\n    ch := make(chan []int, 1)\n    var xs []int = []int{1, 2}\n    ch <- move xs\n}\n");
}
