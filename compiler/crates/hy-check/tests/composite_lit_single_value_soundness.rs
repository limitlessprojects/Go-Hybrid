//! composite_lit_single_value_soundness.rs — regression tests for composite literal
//! item, key, value, and struct field single-value context soundness under
//! Row 13 (BUILDABLE) and HYB-E1002 (issue #2500).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got verdict {:?}, diags {:?}",
        what,
        r.verdict,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {}", what);
}

fn assert_ok(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected clean accept for {}, got errors: {:?}",
        what,
        error_codes(&r)
    );
}

#[test]
fn slice_literal_void_call_element_is_rejected() {
    let src = "fn void() {}\n\nfn main() {\n    xs := []int{void()}\n    fmt.println(\"{len(xs)}\")\n}\n";
    assert_rejects_e1002(src, "[]int{void()}");
}

#[test]
fn array_literal_void_call_element_is_rejected() {
    let src = "fn void() {}\n\nfn main() {\n    arr := [1]int{void()}\n    fmt.println(\"{len(arr)}\")\n}\n";
    assert_rejects_e1002(src, "[1]int{void()}");
}

#[test]
fn map_literal_void_call_key_and_value_are_rejected() {
    let src_key = "fn void() {}\n\nfn main() {\n    m := map[int]int{void(): 1}\n    fmt.println(\"{len(m)}\")\n}\n";
    assert_rejects_e1002(src_key, "map[int]int{void(): 1}");

    let src_val = "fn void() {}\n\nfn main() {\n    m := map[int]int{1: void()}\n    fmt.println(\"{len(m)}\")\n}\n";
    assert_rejects_e1002(src_val, "map[int]int{1: void()}");
}

#[test]
fn struct_literal_void_and_multi_value_fields_are_rejected() {
    let src_void = "fn void() {}\n\nstruct s {\n    x int\n}\n\nfn main() {\n    rec := s{x: void()}\n    fmt.println(\"{rec.x}\")\n}\n";
    assert_rejects_e1002(src_void, "s{x: void()}");

    let src_multi = "fn two() (int, int) {\n    return 1, 2\n}\n\nstruct s {\n    x int\n}\n\nfn main() {\n    rec := s{x: two()}\n    fmt.println(\"{rec.x}\")\n}\n";
    assert_rejects_e1002(src_multi, "s{x: two()}");
}

#[test]
fn nested_untyped_struct_literal_in_slice_checks_single_value() {
    let src = "fn void() {}\n\nstruct s {\n    x int\n}\n\nfn main() {\n    xs := []s{{x: void()}}\n    fmt.println(\"{len(xs)}\")\n}\n";
    assert_rejects_e1002(src, "[]s{{x: void()}}");
}

#[test]
fn valid_composite_literals_stay_cleanly_accepted() {
    let src1 = "fn single() int {\n    return 42\n}\n\nstruct s {\n    x int\n}\n\nfn main() {\n    xs := []int{1, single(), 3}\n    arr := [2]int{single(), 2}\n    m := map[int]string{1: \"one\", single(): \"forty-two\"}\n    rec := s{x: single()}\n    fmt.println(\"{len(xs)} {len(arr)} {len(m)} {rec.x}\")\n}\n";
    assert_ok(src1, "valid composite literals with single values");
}
