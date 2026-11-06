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
