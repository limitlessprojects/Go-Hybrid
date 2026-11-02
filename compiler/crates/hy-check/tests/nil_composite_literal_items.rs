//! nil in composite-literal element / key / value slots whose type Go cannot
//! initialise with `nil` must be HYB-E1002 (BUILDABLE), exactly once.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(body: &str) -> Vec<String> {
    let src = format!("fn main() {{\n{body}\n}}\n");
    let ast = parse_file("test.hy", &src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

/// Exactly one rejection (E1002 or the pre-existing N1 E4001), never both.
fn assert_one_e1002(body: &str) {
    let cs = codes(body);
    assert_eq!(cs.len(), 1, "expected one diagnostic, got {cs:?} for:\n{body}");
    assert!(
        cs[0] == "HYB-E1002" || cs[0] == "HYB-E4001",
        "unexpected {cs:?} for:\n{body}"
    );
}

fn assert_ok(body: &str) {
    let cs = codes(body);
    assert!(
        !cs.contains(&"HYB-E1002".to_string()),
        "unexpected HYB-E1002 in {cs:?} for:\n{body}"
    );
}

#[test]
fn rejects_nil_in_int_slice() {
    assert_one_e1002("    s := []int{nil}\n    _ = s");
}

#[test]
fn rejects_nil_in_bool_slice() {
    assert_one_e1002("    s := []bool{nil}\n    _ = s");
}

#[test]
fn rejects_nil_in_string_array() {
    assert_one_e1002("    s := [1]string{nil}\n    _ = s");
}

#[test]
fn rejects_each_nil_in_array() {
    let cs = codes("    s := [2]string{nil, nil}\n    _ = s");
    assert_eq!(cs.len(), 2, "got {cs:?}");
}

#[test]
fn rejects_parenthesized_nil() {
    assert_one_e1002("    s := []int{(nil)}\n    _ = s");
}

#[test]
fn rejects_nil_map_value() {
    assert_one_e1002("    m := map[string]int{\"k\": nil}\n    _ = m");
}

#[test]
fn rejects_nil_map_key() {
    assert_one_e1002("    m := map[int]int{nil: 1}\n    _ = m");
}

#[test]
fn rejects_nil_in_nested_untyped_slice() {
    assert_one_e1002("    s := [][]int{{nil}}\n    _ = s");
}

#[test]
fn accepts_nil_in_nilable_slots() {
    assert_ok("    a := []*int{nil}\n    _ = a");
    assert_ok("    b := [][]int{nil}\n    _ = b");
    assert_ok("    c := []error{nil}\n    _ = c");
    assert_ok("    d := []any{nil}\n    _ = d");
    assert_ok("    e := map[string][]int{\"k\": nil}\n    _ = e");
    assert_ok("    f := map[string]*int{\"k\": nil}\n    _ = f");
    assert_ok("    g := map[*int]int{nil: 1}\n    _ = g");
    assert_ok("    h := [2]map[string]int{nil, nil}\n    _ = h");
}
