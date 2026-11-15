//! Regression tests for #3729:
//! 1. `static_bind_source_bound` must resolve nested array/map index bases
//!    reached through pointer-to-array (`p[0][0]`, `p[0]["k"]`) via
//!    `composite_type_expr` (`HYB-E2002`).
//! 2. `resolve_fn_callee_ty` must fall back to `method_decls` for user-defined
//!    struct methods in `Expr::Field` (`b.get_pair_fn()(3, 9)`,
//!    `[b.pair][0](3, 9)`) (`HYB-E2002`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_only_e2002(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert_eq!(cs, vec!["HYB-E2002"], "expected only [HYB-E2002], got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn ptr_array_nested_index_and_method_value_callees_reject_e2002() {
    assert_only_e2002(
        "fn check_arr(p *[2][2]int) {\n    a, b := p[0][0]\n    _ = a\n    _ = b\n}\nfn main() {}\n",
    );
    assert_only_e2002(
        "fn check_map(p *[2]map[string]int) {\n    a, b, c := p[0][\"k\"]\n    _ = a\n    _ = b\n    _ = c\n}\nfn main() {}\n",
    );
    assert_only_e2002(
        "struct holder { base int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (h holder) get_pair_fn() fn(a int, b int) int int { return minmax }\nfn main() {\n    h := holder{base: 1}\n    x := h.get_pair_fn()(3, 9)\n    _ = x\n}\n",
    );
    assert_only_e2002(
        "struct holder { base int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (h holder) get_pair_fn() fn(a int, b int) int int { return minmax }\nfn main() {\n    boxes := []holder{holder{base: 1}}\n    x := boxes[0:1][0].get_pair_fn()(3, 9)\n    _ = x\n}\n",
    );
}

#[test]
fn ptr_array_index_and_method_value_destructuring_accept() {
    assert_accept(
        "struct holder { base int }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn (h holder) get_pair_fn() fn(a int, b int) int int { return minmax }\nfn run(p *[2][2]int, mp *[2]map[string]int) {\n    v := p[0][0]\n    mv, ok := mp[0][\"k\"]\n    h := holder{base: v}\n    a, b := h.get_pair_fn()(mv, 9)\n    _ = ok\n    _ = a\n    _ = b\n}\nfn main() {}\n",
    );
}
