//! Issue #1016 — void calls inside map literals, and multi-value calls used
//! as one value in a literal or unary operand, must reject with HYB-E1002.
//!
//! Map keys and values never called `check_single_value_position`, so a void
//! call or zero-value builtin (`delete`) was accepted and then failed `go build`
//! with `(no value) used as value`. A multi-value call in a unary operand or a
//! slice element is the same single-value breach (`multiple-value ... in
//! single-value context`). A one-name binding whose initializer contains a
//! multi-value call also co-emits HYB-E2002 (T23); that co-emission is allowed.
//! Single-value calls in composite literals stay accepted.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

/// A 1-name binding whose initializer contains a multi-value call co-emits
/// HYB-E2002 alongside this rule's HYB-E1002. The program must reject, E1002
/// must be present, and no other code may appear.
fn assert_operand_rejected(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == want),
        "expected [{want}] among the diagnostics, got {cs:?} for:\n{src}"
    );
    assert!(
        cs.iter().all(|c| c == want || c == "HYB-E2002"),
        "unexpected diagnostic besides [{want}]/E2002: {cs:?} for:\n{src}"
    );
}

const TWO: &str = "fn two() (int, int) {\n    return 1, 2\n}\n\n";

#[test]
fn void_call_in_map_value_rejects() {
    assert_code(
        "fn noop() {}\n\nfn main() {\n    m := map[string]int{\"k\": noop()}\n    _ = m\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn void_call_in_map_key_rejects() {
    assert_code(
        "fn noop() {}\n\nfn main() {\n    m := map[int]int{noop(): 1}\n    _ = m\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn delete_builtin_in_map_value_rejects() {
    assert_code(
        "fn main() {\n    m0 := map[string]int{\"a\": 1}\n    m := map[string]int{\"k\": delete(m0, \"a\")}\n    _ = m\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn multivalue_call_as_unary_operand_rejects() {
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    v := -two()\n    _ = v\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn multivalue_call_in_slice_literal_rejects() {
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    xs := []int{{two()}}\n    _ = xs\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn single_value_calls_in_composite_literals_stay_accepted() {
    assert_ok(
        "import \"fmt\"\n\nstruct box { x int }\n\nfn one() int {\n    return 1\n}\n\nfn main() {\n    b := box{x: one()}\n    xs := []int{one()}\n    m := map[int]int{one(): one()}\n    fmt.println(\"{b.x} {len(xs)} {len(m)}\")\n}\n",
    );
}
