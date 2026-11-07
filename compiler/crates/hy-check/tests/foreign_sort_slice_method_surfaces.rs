//! Tests for curated foreign method surfaces of standard library sortable slice handles:
//! `sort.string_slice`, `sort.int_slice`, and `sort.float64_slice`.
//! Guarantees that calling phantom/unsupported methods rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_sort_string_slice_methods_are_rejected() {
    let bad = check(
        "import \"sort\"\n\
         struct holder {\n\
             s sort.string_slice\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             h.s.totally_fake()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(
        bad.verdict,
        Verdict::Reject,
        "expected reject, got {:?}",
        bad.diagnostics
    );
    assert!(
        codes(&bad).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&bad)
    );
    let diag = bad
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E1002")
        .unwrap();
    assert!(
        diag.message.contains("sort.string_slice"),
        "message should name sort.string_slice: {}",
        diag.message
    );
    assert!(
        diag.message.contains("totally_fake"),
        "message should name the missing method: {}",
        diag.message
    );
}

#[test]
fn phantom_sort_int_and_float_slice_methods_are_rejected() {
    let bad_int = check(
        "import \"sort\"\n\
         fn inspect(i *sort.int_slice) {\n\
             i.bogus_op()\n\
         }\n",
    );
    assert_eq!(bad_int.verdict, Verdict::Reject);
    assert!(codes(&bad_int).iter().any(|c| c == "HYB-E1002"));

    let bad_float = check(
        "import \"sort\"\n\
         fn inspect(f *sort.float64_slice) {\n\
             f.non_existent_method()\n\
         }\n",
    );
    assert_eq!(bad_float.verdict, Verdict::Reject);
    assert!(codes(&bad_float).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn package_level_sort_functions_called_as_methods_are_rejected() {
    // sort.slice, sort.slice_stable, sort.ints, and sort.strings are package-level functions, NOT methods on slices
    let bad_slice = check(
        "import \"sort\"\n\
         fn inspect(s *sort.string_slice) {\n\
             s.slice()\n\
         }\n",
    );
    assert_eq!(bad_slice.verdict, Verdict::Reject);
    assert!(codes(&bad_slice).iter().any(|c| c == "HYB-E1002"));

    let bad_ints = check(
        "import \"sort\"\n\
         fn inspect(i *sort.int_slice) {\n\
             i.ints()\n\
         }\n",
    );
    assert_eq!(bad_ints.verdict, Verdict::Reject);
    assert!(codes(&bad_ints).iter().any(|c| c == "HYB-E1002"));
}

#[test]
fn real_sort_slice_methods_accept() {
    let good = check(
        "import \"sort\"\n\
         struct holder {\n\
             s sort.string_slice\n\
             i sort.int_slice\n\
             f sort.float64_slice\n\
         }\n\
         fn run(h *holder) {\n\
             h.s.len()\n\
             h.s.sort()\n\
             h.s.search(\"test\")\n\
             h.i.len()\n\
             h.i.sort()\n\
             h.i.search(42)\n\
             h.f.len()\n\
             h.f.sort()\n\
             h.f.search(3.14)\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}

#[test]
fn all_real_pointer_sort_slice_methods_accept() {
    let good = check(
        "import \"sort\"\n\
         fn inspect_strs(s *sort.string_slice) {\n\
             s.len()\n\
             s.less(0, 1)\n\
             s.swap(0, 1)\n\
             s.sort()\n\
             s.search(\"query\")\n\
         }\n\
         fn inspect_ints(i *sort.int_slice) {\n\
             i.len()\n\
             i.less(0, 1)\n\
             i.swap(0, 1)\n\
             i.sort()\n\
             i.search(100)\n\
         }\n\
         fn inspect_flts(f *sort.float64_slice) {\n\
             f.len()\n\
             f.less(0, 1)\n\
             f.swap(0, 1)\n\
             f.sort()\n\
             f.search(1.618)\n\
         }\n",
    );
    assert_eq!(
        good.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        good.diagnostics
    );
}
