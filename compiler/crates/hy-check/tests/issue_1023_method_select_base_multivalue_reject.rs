//! Issue #1023 — a multi-value call used as a method-selection base.
//!
//! Go refuses `two().run()` because the call is not one value. A 0-value
//! call in the same position (`noop().run()`, `panic("abort").run()`) is
//! the same violation. A one-result call that returns a struct stays
//! accepted for a method call.

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
fn void_call_as_method_selection_base_rejects() {
    assert_code(
        "fn noop() {}\n\nfn main() {\n    noop().run()\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\n    panic(\"abort\").run()\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn multivalue_call_as_method_selection_base_rejects() {
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    two().run()\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn single_value_method_call_stays_accepted() {
    assert_ok(
        "import \"fmt\"\n\nstruct item { v int }\n\nfn (i item) get() int {\n    return i.v\n}\n\nfn make_item() item {\n    return item{v: 42}\n}\n\nfn main() {\n    a := make_item().v\n    b := make_item().get()\n    fmt.println(\"{a} {b}\")\n}\n",
    );
}
