//! #682 U6/U7/U8 — a call used where exactly one value is required.
//!
//! Three programs that `hyb check` accepted with 0 diagnostics while the emitted
//! Go failed `go build`, violating `GUARANTEES.md` Row 13 (BUILDABLE):
//!
//!   * **U6** — a multi-value call as an operand: `two() + 1` is Go's
//!     `multiple-value two() (value of type (int, int)) in single-value
//!     context`.
//!   * **U7** — a multi-value call as the whole argument list of a callee that
//!     cannot take that many: `g(two())` for a 1-param `g` is
//!     `too many arguments in call to g`.
//!   * **U8** — a call with no results used as a value: `a := v()` is
//!     `v() (no value) used as value`.
//!
//! All three share one root cause: `kind_of_expr` on a call returned the FIRST
//! result kind for a multi-value call (silently discarding the rest) and
//! `OpKind::Void` for a no-result call (which no operand check rejected). The
//! fix derives the call's value count in one place
//! (`non_single_value_count`) and rejects it in the positions that need one.
//!
//! The accept cases are the point of this file, because the legal multi-value
//! forms are easy to break: `a, b := f()`, `a, b = f()`, `return f()` with
//! matching results, `g(f())` where `g` really takes that many, and the two
//! non-call multi-value sources (`m[k]` and `<-ch`) all must keep working.

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

/// U6 operand variant. The PR's strict all-equal assertion was written
/// against a base without the landed T23 multi-value binding rule
/// (pr-599/#669, `HYB-E2002`): on the merged tree a 1-name binding whose
/// initializer CONTAINS a multi-value call co-emits E2002 alongside this
/// rule's E1002. The program's verdict (REJECT) and E1002's presence are
/// asserted strictly; E2002 is the documented landed co-emission.
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

// ---------------------------------------------------------------------------
// U6 — a multi-value call as an operand
// ---------------------------------------------------------------------------

#[test]
fn multi_value_call_as_an_operand_is_rejected() {
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    v := two() + 1\n    fmt.println(\"{{v}}\")\n}}\n"),
        "HYB-E1002",
    );
    // Either side, and the other binary operators.
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    v := 1 + two()\n    fmt.println(\"{{v}}\")\n}}\n"),
        "HYB-E1002",
    );
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    v := two() * 2\n    fmt.println(\"{{v}}\")\n}}\n"),
        "HYB-E1002",
    );
    // An ordered comparison is a single-value context too.
    assert_operand_rejected(
        &format!("{TWO}fn main() {{\n    if two() > 1 {{\n        fmt.println(\"gt\")\n    }}\n}}\n"),
        "HYB-E1002",
    );
}

// ---------------------------------------------------------------------------
// U7 — a multi-value call as the whole argument list
// ---------------------------------------------------------------------------

#[test]
fn multi_value_call_for_a_smaller_callee_is_rejected() {
    assert_code(
        &format!("{TWO}fn g(a int) {{\n    fmt.println(\"{{a}}\")\n}}\n\nfn main() {{\n    g(two())\n}}\n"),
        "HYB-E1002",
    );
    // Three results into a 2-param callee.
    assert_code(
        "fn three() (int, int, int) {\n    return 1, 2, 3\n}\n\nfn g(a int, b int) {\n    fmt.println(\"{a} {b}\")\n}\n\nfn main() {\n    g(three())\n}\n",
        "HYB-E1002",
    );
    // Two results into a 3-param callee.
    assert_code(
        &format!("{TWO}fn g(a int, b int, c int) {{\n    fmt.println(\"{{a}} {{b}} {{c}}\")\n}}\n\nfn main() {{\n    g(two())\n}}\n"),
        "HYB-E1002",
    );
}

#[test]
fn multi_value_call_for_a_matching_callee_is_accepted() {
    // This is legal Go: the lone multi-value call IS the whole argument list.
    assert_ok(
        &format!("{TWO}fn g(a int, b int) {{\n    fmt.println(\"{{a}} {{b}}\")\n}}\n\nfn main() {{\n    g(two())\n}}\n"),
    );
}

// ---------------------------------------------------------------------------
// U8 — a no-result call used as a value
// ---------------------------------------------------------------------------

#[test]
fn no_result_call_used_as_a_value_is_rejected() {
    assert_code(
        "fn v() {\n    fmt.println(\"x\")\n}\n\nfn main() {\n    a := v()\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E1002",
    );
    // Assignment (not just binding).
    assert_code(
        "fn v() {\n    fmt.println(\"x\")\n}\n\nfn main() {\n    var a int = 1\n    a = v()\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E1002",
    );
    // As a call argument and as an operand.
    assert_code(
        "fn v() {\n    fmt.println(\"x\")\n}\n\nfn g(a int) {\n    fmt.println(\"{a}\")\n}\n\nfn main() {\n    g(v())\n}\n",
        "HYB-E1002",
    );
    assert_code(
        "fn v() {\n    fmt.println(\"x\")\n}\n\nfn main() {\n    a := v() + 1\n    fmt.println(\"{a}\")\n}\n",
        "HYB-E1002",
    );
}

#[test]
fn no_result_call_as_a_statement_stays_accepted() {
    // The whole point of a no-result function: call it on its own line.
    assert_ok("fn v() {\n    fmt.println(\"x\")\n}\n\nfn main() {\n    v()\n}\n");
    // A `defer` of it, and a task spawn.
    assert_ok("fn v() {\n    fmt.println(\"x\")\n}\n\nfn main() {\n    defer v()\n}\n");
}

// ---------------------------------------------------------------------------
// the legal multi-value forms
// ---------------------------------------------------------------------------

#[test]
fn legal_multi_value_bindings_stay_accepted() {
    assert_ok(&format!("{TWO}fn main() {{\n    a, b := two()\n    fmt.println(\"{{a}} {{b}}\")\n}}\n"));
    assert_ok(&format!(
        "{TWO}fn main() {{\n    var a int\n    var b int\n    a, b = two()\n    fmt.println(\"{{a}} {{b}}\")\n}}\n"
    ));
    // `return f()` when the results line up exactly.
    assert_ok(&format!(
        "{TWO}fn fwd() (int, int) {{\n    return two()\n}}\n\nfn main() {{\n    a, b := fwd()\n    fmt.println(\"{{a}} {{b}}\")\n}}\n"
    ));
}

#[test]
fn non_call_multi_value_sources_stay_accepted() {
    // `v, ok := m[k]` and `v, ok := <-ch` are the other two two-value forms and
    // are not calls, so the new rule must not touch them.
    assert_ok(
        "fn main() {\n    m := map[string]int{}\n    v, ok := m[\"k\"]\n    fmt.println(\"{v} {ok}\")\n}\n",
    );
    assert_ok(
        "fn main() {\n    c := make(chan int, 1)\n    c <- 7\n    v, ok := <-c\n    fmt.println(\"{v} {ok}\")\n}\n",
    );
}

#[test]
fn single_value_calls_stay_accepted() {
    assert_ok("fn one() int {\n    return 1\n}\n\nfn main() {\n    a := one()\n    fmt.println(\"{a}\")\n}\n");
    assert_ok("fn one() int {\n    return 1\n}\n\nfn main() {\n    v := one() + 1\n    fmt.println(\"{v}\")\n}\n");
    assert_ok(
        "fn one() int {\n    return 1\n}\n\nfn g(a int) {\n    fmt.println(\"{a}\")\n}\n\nfn main() {\n    g(one())\n}\n",
    );
}

#[test]
fn multi_value_call_as_for_range_expr_is_rejected() {
    let src = "fn two_slices() ([]int, []int) {\n    return []int{1, 2}, []int{3, 4}\n}\n\nfn main() {\n    for x in two_slices() {\n        _ = x\n    }\n}\n";
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for multi-value call in for-range expression, got {cs:?}"
    );
}

#[test]
fn void_call_as_for_range_expr_is_rejected() {
    let src = "fn v() {}\n\nfn main() {\n    for x in v() {\n        _ = x\n    }\n}\n";
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for void call in for-range expression, got {cs:?}"
    );
}

#[test]
fn multi_value_call_as_for_range_bound_is_rejected() {
    let src = "fn two() (int, int) {\n    return 1, 2\n}\n\nfn main() {\n    for i in two()..10 {\n        _ = i\n    }\n}\n";
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for multi-value call in for-range bound, got {cs:?}"
    );
}


