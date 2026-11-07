//! Regression tests for #3734: `addressable.rs` must resolve pointer-to-array
//! index hops (`pm[0]["a"].n = 1`, `n := &pm[0]["a"].n`, `ps[0][0] = 65`,
//! `b := &ps[0][0]`) and reject unaddressable map/string element mutations
//! and O8 slot loans with `HYB-E2010`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_has_e2010(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.contains(&"HYB-E2010"), "expected HYB-E2010, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn ptr_array_map_and_string_element_stores_and_loans_reject_e2010() {
    assert_has_e2010(
        "struct holder { n int }\nfn bad(pm *[2]map[string]holder) {\n    pm[0][\"a\"].n = 1\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "struct holder { n int }\nfn bad(pm *[2]map[string]holder) {\n    n := &pm[0][\"a\"].n\n    _ = n\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "fn bad(ps *[2]string) {\n    ps[0][0] = 65\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "fn bad(ps *[2]string) {\n    b := &ps[0][0]\n    _ = b\n}\nfn main() {}\n",
    );
}

#[test]
fn ptr_array_whole_map_slot_assign_and_loan_accept() {
    assert_accept(
        "struct holder { n int }\nfn ok(pm *[2]map[string]holder) {\n    pm[0][\"a\"] = holder{n: 1}\n    h := &pm[0][\"a\"]\n    h.n = 2\n}\nfn main() {}\n",
    );
}

#[test]
fn ptr_array_index_and_slice_shapes_no_false_e3005_3839() {
    // #3839: `p[0]` on a `*[N]T` must shape as the element T (not
    // Pointer(Array(T))), so `p[0].insert(xs)` on a `set[any]` element and
    // `p[0:2]` / `p[0]` element reads do not trip the E3005 comparability
    // guard with a false Pointer shape.
    assert_accept(
        "import \"fmt\"\nfn run(p *[1]set[any]) {\n    xs := []int{1, 2}\n    p[0].insert(xs)\n    fmt.println(len(xs))\n}\nfn main() {\n    var a [1]set[any]\n    run(&a)\n}\n",
    );
    assert_accept(
        "import \"fmt\"\nfn run(p_arr *[2]int, p_slices *[1][]int) {\n    for x in p_arr[0:2] {\n        fmt.println(x)\n    }\n    for y in p_slices[0] {\n        fmt.println(y)\n    }\n}\nfn main() {\n    var a [2]int\n    var b [1][]int\n    run(&a, &b)\n}\n",
    );
}
