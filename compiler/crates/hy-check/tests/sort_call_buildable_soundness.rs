//! Soundness and buildable validation tests for `sort` calls under Row 13 (BUILDABLE):
//! 1. `sort` core functions require exact arity:
//!    - 1 arg: ints, ints_are_sorted, float64s, float64s_are_sorted, strings, strings_are_sorted.
//!    - 2 args: search, search_ints, search_float64s, search_strings, slice, slice_stable, slice_is_sorted.
//! 2. Arity mismatches must reject fail-closed with `HYB-E1002`.
//! 3. Positive controls: legal calls with proper arity accept cleanly.
//!
//! Settlement Wallet (Base L2 USDC): 0x8366bCe3a2D379De7656D7A67015789FaF999f20

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_sort_one_arg_functions_arity_reject() {
    // ints: takes 1
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.ints()\n}\n");
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.ints([]int{1}, []int{2})\n}\n");

    // strings: takes 1
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.strings()\n}\n");

    // float64s: takes 1
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.float64s()\n}\n");

    // ints_are_sorted: takes 1
    rejects_e1002("import \"sort\"\nfn main() {\n    b := sort.ints_are_sorted()\n    _ = b\n}\n");

    // strings_are_sorted: takes 1
    rejects_e1002("import \"sort\"\nfn main() {\n    b := sort.strings_are_sorted()\n    _ = b\n}\n");
}

#[test]
fn test_sort_two_arg_functions_arity_reject() {
    // search_ints: takes 2
    rejects_e1002("import \"sort\"\nfn main() {\n    idx := sort.search_ints([]int{1, 2})\n    _ = idx\n}\n");
    rejects_e1002("import \"sort\"\nfn main() {\n    idx := sort.search_ints([]int{1, 2}, 1, 2)\n    _ = idx\n}\n");

    // search_strings: takes 2
    rejects_e1002("import \"sort\"\nfn main() {\n    idx := sort.search_strings([]string{\"a\"})\n    _ = idx\n}\n");

    // slice: takes 2
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.slice([]int{1, 2})\n}\n");
    rejects_e1002("import \"sort\"\nfn main() {\n    sort.slice_stable([]int{1, 2})\n}\n");
}

#[test]
fn test_sort_positive_controls_accept() {
    let src = r#"import "sort"

fn main() {
    nums := []int{3, 1, 2}
    sort.ints(nums)

    b1 := sort.ints_are_sorted(nums)
    _ = b1

    strs := []string{"c", "a", "b"}
    sort.strings(strs)

    b2 := sort.strings_are_sorted(strs)
    _ = b2

    flts := []float64{2.5, 1.1}
    sort.float64s(flts)

    b3 := sort.float64s_are_sorted(flts)
    _ = b3

    idx := sort.search_ints(nums, 2)
    _ = idx
}
"#;
    accepts(src);
}
