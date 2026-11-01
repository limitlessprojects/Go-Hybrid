//! Issue #1013 — void calls, 0-value builtins, and multi-value calls in
//! integer range bounds (`lo..hi` / `lo..=hi`) must reject with HYB-E1002.
//!
//! `for i in noop()..10` lowers to `for i := noop(); i < 10; i++`. A void
//! or multi-value call in either bound is `(no value) used as value` or
//! `multiple-value ... in single-value context` in the emitted Go, so
//! `hyb check` must reject it before emission.

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
fn void_and_multivalue_calls_in_range_bounds_reject() {
    assert_code(
        "import \"fmt\"\n\nfn noop() {}\n\nfn main() {\n    for i in noop()..10 {\n        fmt.println(\"{i}\")\n    }\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "import \"fmt\"\n\nfn noop() {}\n\nfn main() {\n    for i in 0..noop() {\n        fmt.println(\"{i}\")\n    }\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "import \"fmt\"\n\nfn main() {\n    ch := make(chan int, 1)\n    for i in 0..=close(ch) {\n        fmt.println(\"{i}\")\n    }\n}\n",
        "HYB-E1002",
    );
    assert_operand_rejected(
        &format!(
            "{TWO}import \"fmt\"\n\nfn main() {{\n    for i in 0..two() {{\n        fmt.println(\"{{i}}\")\n    }}\n}}\n"
        ),
        "HYB-E1002",
    );
    // A single-value call remains a legal integer bound.
    assert_ok(
        "import \"fmt\"\n\nfn bound() int {\n    return 5\n}\n\nfn main() {\n    for i in 0..bound() {\n        fmt.println(\"step={bound()} i={i}\")\n    }\n}\n",
    );
}
