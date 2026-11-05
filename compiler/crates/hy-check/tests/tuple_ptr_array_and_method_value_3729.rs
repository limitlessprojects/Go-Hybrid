//! Regression tests for #3729:
//! 1. `static_bind_source_bound` falls back to `composite_type_expr` with
//!    pointer/alias peeling on `Expr::Index` bases (`p: *[2][2]int` in
//!    `a, b := p[0][0]`, or `p: *[2]map[string]int` in `a, b, c := p[0]["k"]`),
//!    rejecting multi-target arity mismatches with `HYB-E2002`.
//! 2. `resolve_fn_callee_ty` resolves user-defined struct methods returning
//!    function values (`p := b.get_pair_fn()(3, 9)` and
//!    `p := [b.get_pair_fn][0](3, 9)`), rejecting multi-value arity mismatches
//!    with `HYB-E2002`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_only_e2002(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(
        cs,
        vec!["HYB-E2002"],
        "expected only [HYB-E2002], got {cs:?} for:\n{src}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.as_str())
        .collect();
    assert!(
        cs.is_empty(),
        "expected 0 errors, got {cs:?} for:\n{src}"
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn ptr_array_index_bounds_and_method_value_fn_callees_reject_e2002_3729() {
    assert_only_e2002(
        "fn check_arr(p *[2][2]int) {\n    a, b := p[0][0]\n    _ = a\n    _ = b\n}\nfn main() {}\n",
    );
    assert_only_e2002(
        "fn check_map(p *[2]map[string]int) {\n    a, b, c := p[0][\"k\"]\n    _ = a\n    _ = b\n    _ = c\n}\nfn main() {}\n",
    );
    assert_only_e2002(
        "struct box_fn { x int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (b box_fn) get_pair_fn() fn(a int, b int) int int { return minmax }\nfn main() {\n    b := box_fn{x: 1}\n    p := b.get_pair_fn()(3, 9)\n    _ = p\n}\n",
    );
    assert_only_e2002(
        "struct box_fn { x int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (b box_fn) pair(a int, c int) (int, int) { return a, c }\nfn main() {\n    b := box_fn{x: 1}\n    p := [b.pair][0](3, 9)\n    _ = p\n}\n",
    );
}

#[test]
fn ptr_array_index_and_method_value_valid_bindings_accept_3729() {
    assert_accept(
        "struct box_fn { x int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (b box_fn) get_pair_fn() fn(a int, b int) int int { return minmax }\nfn check_ok(p *[2][2]int, m *[2]map[string]int) {\n    a := p[0][0]\n    v, ok := m[0][\"k\"]\n    b := box_fn{x: a}\n    x, y := b.get_pair_fn()(v, 9)\n    _ = ok\n    _ = x\n    _ = y\n}\nfn main() {}\n",
    );
}
