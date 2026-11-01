//! Issue #1023 — a 0-value call used as a field-selection base.
//!
//! Go refuses `noop().name` and `close(ch).name` because the call produces
//! no value. A multi-value call in the same position (`two().name`) is the
//! same single-value violation. A one-result call that returns a struct
//! stays accepted.

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
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

/// A 1-name binding whose initializer contains a multi-value call co-emits
/// HYB-E2002 (T23) alongside this rule's HYB-E1002. The verdict is REJECT
/// and E1002 must be present; E2002 is the documented co-emission.
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
fn void_call_as_field_selection_base_rejects() {
    assert_code(
        "struct box { name string }\n\nfn noop() {}\n\nfn main() {\n    x := noop().name\n    _ = x\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn void_builtins_as_field_selection_bases_reject() {
    assert_code(
        "fn main() {\n    ch := make(chan int, 1)\n    x := close(ch).name\n    _ = x\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    m := map[string]int{\"a\": 1}\n    x := delete(m, \"a\").name\n    _ = x\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    xs := []int{1, 2}\n    x := clear(xs).name\n    _ = x\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn multivalue_call_as_field_selection_base_rejects() {
    assert_operand_rejected(
        &format!("{TWO}struct box {{ name string }}\n\nfn main() {{\n    x := two().name\n    _ = x\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn single_value_struct_field_stays_accepted() {
    assert_ok(
        "import \"fmt\"\n\nstruct item { v int }\n\nfn (i item) get() int {\n    return i.v\n}\n\nfn make_item() item {\n    return item{v: 42}\n}\n\nfn main() {\n    a := make_item().v\n    b := make_item().get()\n    fmt.println(\"{a} {b}\")\n}\n",
    );
}
