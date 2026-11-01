//! index_slice_nil_soundness.rs — regression test for issue #2753:
//! index expressions (slice, string, array, map) and slice bounds (lo:hi)
//! must reject untyped `nil` operands on non-nilable targets (HYB-E1002).

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
fn slice_index_nil_rejects_e1002() {
    let src = "\
fn main() {
    xs := []int{1, 2, 3}
    _ = xs[nil]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for xs[nil], got {:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_index_nil_rejects_e1002() {
    let src = "\
fn main() {
    s := \"hello\"
    _ = s[nil]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for s[nil], got {:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn array_index_nil_rejects_e1002() {
    let src = "\
fn main() {
    arr := [2]int{1, 2}
    _ = arr[nil]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for arr[nil], got {:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn slice_bounds_nil_rejects_e1002() {
    let src_lo = "\
fn main() {
    xs := []int{1, 2, 3}
    _ = xs[nil:2]
}
";
    let r_lo = check(src_lo);
    assert_eq!(r_lo.verdict, Verdict::Reject, "expected reject for xs[nil:2], got {:?}", r_lo.diagnostics);
    assert!(error_codes(&r_lo).contains(&"HYB-E1002".to_string()));

    let src_hi = "\
fn main() {
    xs := []int{1, 2, 3}
    _ = xs[0:nil]
}
";
    let r_hi = check(src_hi);
    assert_eq!(r_hi.verdict, Verdict::Reject, "expected reject for xs[0:nil], got {:?}", r_hi.diagnostics);
    assert!(error_codes(&r_hi).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_non_nilable_key_nil_rejects_e1002() {
    let src_int = "\
fn main() {
    m := map[int]string{}
    _ = m[nil]
}
";
    let r_int = check(src_int);
    assert_eq!(r_int.verdict, Verdict::Reject, "expected reject for map[int]string[nil], got {:?}", r_int.diagnostics);
    assert!(error_codes(&r_int).contains(&"HYB-E1002".to_string()));

    let src_str = "\
fn main() {
    m := map[string]int{}
    _ = m[nil]
}
";
    let r_str = check(src_str);
    assert_eq!(r_str.verdict, Verdict::Reject, "expected reject for map[string]int[nil], got {:?}", r_str.diagnostics);
    assert!(error_codes(&r_str).contains(&"HYB-E1002".to_string()));
}

#[test]
fn legal_nilable_map_key_nil_accepts() {
    let src = "\
fn main() {
    m := map[*int]string{}
    _ = m[nil]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept for map[*int]string[nil], got diagnostics: {:?}", r.diagnostics);
}
