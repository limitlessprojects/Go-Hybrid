//! Regression tests for `HYB-E2002` (`tuple.rs`):
//! 1. Folded identifier-indexed function calls (`fns[idx](3, 9)`, `m[key](3, 9)`,
//!    `h.fns[key](3, 9)`, `h.cbs[key](minmax(3, 9))`) and file-level `const` fn aliases.
//! 2. Builtin calls, predeclared conversions, and associated functions receiving a
//!    lone multi-value argument with mismatched parameter count.
//! 3. Error-tail multi-result `return` statements (`(int, error)`), `if`-initializer
//!    closure/alias bindings, and first-class associated-function values in untyped slices.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e2002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(cs.iter().any(|c| c == "HYB-E2002"), "expected HYB-E2002 in {cs:?}");
}

fn assert_only_e2002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert_eq!(error_codes(&r), vec!["HYB-E2002".to_string()], "for:\n{src}");
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "unexpected errors: {:?} for:\n{src}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn folded_ident_indexed_and_const_aliased_multi_value_calls_reject_e2002() {
    for src in [
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn run(idx int) int {\n fns := []fn(a int, b int) int int{minmax}\n p := fns[idx](3, 9)\n return p\n}\nfn main() { _ = run(0) }\n",
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn run(m map[string]fn(a int, b int) int int, key string) int {\n p := m[key](3, 9)\n return p\n}\nfn main() {}\n",
        "struct holder { fns map[string]fn(a int, b int) int int }\nfn run(h holder, key string) int {\n p := h.fns[key](3, 9)\n return p\n}\nfn main() {}\n",
        "struct holder { cbs map[string]fn(x int) }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn run(h holder, key string) {\n h.cbs[key](minmax(3, 9))\n}\nfn main() {}\n",
        "fn minmax(a int, b int) (int, int) { return a, b }\nconst mm = minmax\nfn main() { p := mm(3, 9)\n _ = p }\n",
        "fn minmax(a int, b int) (int, int) { return a, b }\nconst cb = fn(x int) { _ = x }\nfn main() { cb(minmax(3, 9)) }\n",
    ] {
        assert_only_e2002(src);
    }
}

#[test]
fn builtins_conversions_and_associated_functions_reject_mismatched_multi_value_forwarding_e2002() {
    for src in [
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn main() { panic(minmax(3, 9)) }\n",
        "fn two_slices() ([]int, []int) { return []int{1}, []int{2} }\nfn main() { n := len(two_slices())\n _ = n }\n",
        "fn two_slices() ([]int, []int) { return []int{1}, []int{2} }\nfn main() { n := cap(two_slices())\n _ = n }\n",
        "fn two_maps() (map[string]int, map[string]int) { return map[string]int{}, map[string]int{} }\nfn main() { clear(two_maps()) }\n",
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn main() { x := int64(minmax(3, 9))\n _ = x }\n",
        "fn three_floats() (f64, f64, f64) { return 1.0, 2.0, 3.0 }\nfn main() { z := complex(three_floats())\n _ = z }\n",
        "struct box { v int }\nfn box.take_one(x int) { _ = x }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn main() { box.take_one(minmax(3, 9)) }\n",
        "struct box { v int }\nfn box.take_one(x int) { _ = x }\nfn minmax(a int, b int) (int, int) { return a, b }\nfn main() { task box.take_one(minmax(3, 9)) }\n",
    ] {
        assert_only_e2002(src);
    }

    assert_accept(
        "struct box { v int }\n\
         fn box.take_two(a int, b int) { _ = a\n _ = b }\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn two_floats() (f64, f64) { return 1.0, 2.0 }\n\
         fn main() {\n\
             z := complex(two_floats())\n\
             _ = z\n\
             box.take_two(minmax(3, 9))\n\
             task box.take_two(minmax(3, 9))\n\
         }\n",
    );
}

#[test]
fn error_tail_returns_if_init_closures_and_untyped_method_slices_reject_e2002() {
    for src in [
        "struct box { v int }\nfn box.triple() (int, int, int) { return 1, 2, 3 }\nfn bad() (int, error) { return box.triple() }\nfn main() {}\n",
        "fn triple() (int, int, int) { return 1, 2, 3 }\nfn bad() (int, error) {\n fns := []fn() int int int{triple}\n return fns[0]()\n}\nfn main() {}\n",
        "struct box { v int }\nfn box.pair() (int, int) { return 1, 2 }\nconst fns = [box.pair]\nfn main() { p := fns[0]()\n _ = p }\n",
    ] {
        assert_only_e2002(src);
    }
    assert_e2002("fn main() {\n if cb := fn() (int, int) { return 1, 2 }; cb() == 1 {}\n}\n");
    assert_e2002("fn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n if mm := minmax; mm(3, 9) == 3 {}\n}\n");
}
