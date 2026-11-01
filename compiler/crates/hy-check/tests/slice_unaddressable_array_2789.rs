//! slice_unaddressable_array_2789.rs — issue #2789 (Row 13 BUILDABLE).
//!
//! Go's Slice-expressions rule: *"If the sliced operand is an array, it must
//! be addressable."* A composite literal and a map index expression are never
//! addressable, so slicing an array-typed one passed `hyb check` with 0
//! diagnostics and failed `go build` with `cannot slice unaddressable value
//! ... (value of type [3]int)`.
//!
//! A call result is deliberately fail-open: the emitter lowers an
//! array-returning call to an addressable temporary, so `clone_arr(a)[:]`
//! builds —
//! `tests/addr_slice_and_multireturn_borrow_soundness.rs::array_call_copy_control`
//! pins that as accepted, and this file keeps it pinned.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn slicing_unaddressable_array_values_rejects() {
    // The issue's own reproducer, minus the call-result form (fail-open).
    let r = check(
        "fn main() {\n    _ = [3]int{1, 2, 3}[0:2]\n    m := map[string][3]int{ \"a\": [3]int{1, 2, 3} }\n    _ = m[\"a\"][:2]\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected at least two HYB-E1002, got {codes:?}"
    );

    for (name, src) in [
        (
            "parenthesized composite literal",
            "fn main() {\n    _ = ([3]int{1, 2, 3})[0:2]\n}\n",
        ),
        (
            "parenthesized map element",
            "fn main() {\n    m := map[string][3]int{ \"a\": [3]int{1, 2, 3} }\n    _ = (m[\"a\"])[:2]\n}\n",
        ),
        (
            "open-ended slice of a map element",
            "fn main() {\n    m := map[string][3]int{ \"a\": [3]int{1, 2, 3} }\n    _ = m[\"a\"][:]\n}\n",
        ),
        (
            "map element in a value position",
            "fn take(xs []int) int {\n    return len(xs)\n}\nfn main() {\n    m := map[string][3]int{ \"a\": [3]int{1, 2, 3} }\n    _ = take(m[\"a\"][1:])\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "{name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "{name}: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn addressable_arrays_call_results_and_slice_or_string_bases_stay_accepted() {
    for (name, src) in [
        (
            "array variable",
            "fn main() {\n    a := [3]int{1, 2, 3}\n    _ = a[0:2]\n}\n",
        ),
        (
            "array parameter",
            "fn take(a [3]int) []int {\n    return a[0:2]\n}\nfn main() {\n    _ = take\n}\n",
        ),
        (
            "array indexing of an addressable array",
            "fn main() {\n    a := [2][3]int{ [3]int{1, 2, 3}, [3]int{4, 5, 6} }\n    _ = a[0][0:2]\n}\n",
        ),
        (
            "struct field array of an addressable struct",
            "struct s {\n    a [3]int\n}\nfn main() {\n    v := s{ a: [3]int{1, 2, 3} }\n    _ = v.a[0:2]\n}\n",
        ),
        (
            "array-returning call result (emitter makes it addressable)",
            "fn clone_arr(a [2]int) [2]int {\n    return a\n}\nfn main() {\n    var a [2]int\n    _ = clone_arr(a)[:]\n}\n",
        ),
        (
            "slice literal, call result and map element of slice type",
            "fn sl() []int {\n    return []int{1, 2, 3}\n}\nfn main() {\n    _ = []int{1, 2, 3}[0:2]\n    _ = sl()[1:]\n    m := map[string][]int{ \"a\": []int{1, 2, 3} }\n    _ = m[\"a\"][:2]\n}\n",
        ),
        (
            "string bases",
            "fn s() string {\n    return \"abc\"\n}\nfn main() {\n    _ = \"abc\"[0:2]\n    _ = s()[1:]\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
        assert!(error_codes(&r).is_empty(), "{name}: {:?}", r.diagnostics);
    }
}
