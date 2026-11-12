//! naked_return_shadowed_result.rs — HYB-E1002 for a naked `return` while a
//! named result is shadowed by an inner-scope binding (Row 13 BUILDABLE).
//!
//! hy-emit lowers a value-less `return` in a function whose results are all
//! named to Go's naked `return`. Go refuses that while a different entity
//! with the same name as a result is in scope:
//! `result parameter a not in scope at return`. These programs used to pass
//! `hyb check` with 0 diagnostics and then fail `go build`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn shadow_diags(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .filter(|d| d.code == "HYB-E1002" && d.message.contains("not in scope at return"))
        .count()
}

fn assert_rejects(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject: {:#?}", r.diagnostics);
    assert!(shadow_diags(&r) >= 1, "expected shadowed-result HYB-E1002: {:#?}", r.diagnostics);
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept: {:#?}", r.diagnostics);
}

// -- reject ------------------------------------------------------------------

#[test]
fn define_in_if_block_shadows_result() {
    assert_rejects(
        "fn f() (a int, err error) {\n    if true {\n        a := 2\n        fmt.println(a)\n        return\n    }\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn var_in_nested_block_shadows_error_result() {
    assert_rejects(
        "fn f() (n int, err error) {\n    {\n        var err error\n        fmt.println(err)\n        return\n    }\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn local_const_in_loop_shadows_result() {
    assert_rejects(
        "fn f() (n int) {\n    for i in 0..3 {\n        const n = 4\n        fmt.println(n, i)\n        return\n    }\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn read_for_binder_shadows_result() {
    assert_rejects(
        "fn f(xs []int) (a int) {\n    for a in xs {\n        fmt.println(a)\n        return\n    }\n    return\n}\nfn main() { fmt.println(f([]int{1})) }\n",
    );
}

#[test]
fn if_init_comma_ok_shadows_result() {
    assert_rejects(
        "fn f(m map[string]int) (v int) {\n    if v, ok := m[\"k\"]; ok {\n        fmt.println(v)\n        return\n    }\n    return\n}\nfn main() { fmt.println(f(map[string]int{})) }\n",
    );
}

#[test]
fn closure_naked_return_sees_its_own_shadowed_result() {
    assert_rejects(
        "fn main() {\n    g := fn() (k int) {\n        if true {\n            k := 1\n            fmt.println(k)\n            return\n        }\n        return\n    }\n    fmt.println(g())\n}\n",
    );
}

// -- accept controls ---------------------------------------------------------

#[test]
fn shadow_closed_before_naked_return_accepts() {
    assert_accepts(
        "fn f() (a int, err error) {\n    if true {\n        a := 2\n        fmt.println(a)\n    }\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn explicit_return_under_shadow_accepts() {
    assert_accepts(
        "fn f() (a int, err error) {\n    if true {\n        a := 2\n        return a, nil\n    }\n    a = 3\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn assignment_to_result_in_inner_block_accepts() {
    assert_accepts(
        "fn f() (a int, err error) {\n    if true {\n        b := 2\n        a = b\n        return\n    }\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn untouched_for_binder_is_elided_and_accepts() {
    // An unread `for` binder is emitted as `_` (#139), so nothing shadows.
    assert_accepts(
        "fn f(xs []int) (a int) {\n    for a in xs {\n        return\n    }\n    return\n}\nfn main() { fmt.println(f([]int{1})) }\n",
    );
}

#[test]
fn closure_rebinding_outer_result_name_accepts() {
    // The closure's naked `return` refers to the closure's own results; the
    // outer `n` being shadowed inside it is irrelevant.
    assert_accepts(
        "fn f() (n int) {\n    g := fn() (k int) {\n        n := 3\n        k = n\n        return\n    }\n    n = g()\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}

#[test]
fn closure_with_same_result_name_accepts() {
    assert_accepts(
        "fn f() (n int) {\n    g := fn() (n int) {\n        n = 3\n        return\n    }\n    n = g()\n    return\n}\nfn main() { fmt.println(f()) }\n",
    );
}
