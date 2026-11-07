//! Regression tests for #3734:
//! 1. `place_unassignable_at` and `place_unaddressable_at` resolve map and
//!    string element stores reached through pointer-to-array indexing
//!    (`pm: *[2]map[string]holder` in `pm[0]["a"].n = 1` and `ps: *[2]string`
//!    in `ps[0][0] = 65`) and reject them with `HYB-E2010`.
//! 2. `expr_unassignable_at` and `expr_unaddressable_at` resolve O8 slot-loan
//!    operands reached through pointer-to-array indexing
//!    (`n := &pm[0]["a"].n`, `b := &ps[0][0]`) and reject them with `HYB-E2010`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn ptr_array_index_hops_for_map_and_string_stores_and_loans_reject_e2010_3734() {
    let cases = [
        "struct holder { n int }\nfn mutate_map_field(pm *[2]map[string]holder) {\n    pm[0][\"a\"].n = 1\n}\nfn main() {}\n",
        "fn mutate_str_byte(ps *[2]string) {\n    ps[0][0] = 65\n}\nfn main() {}\n",
        "struct holder { n int }\nfn loan_map_field(pm *[2]map[string]holder) {\n    n := &pm[0][\"a\"].n\n    *n = 1\n}\nfn main() {}\n",
        "fn loan_str_byte(ps *[2]string) {\n    b := &ps[0][0]\n    *b = 65\n}\nfn main() {}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E2010"),
            "expected HYB-E2010 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn ptr_array_whole_map_slot_assign_and_loan_accept_3734() {
    let src = "\
struct holder {
    n int
}

fn update_ok(pm *[2]map[string]holder) {
    pm[0][\"a\"] = holder{n: 1}
    h := &pm[0][\"a\"]
    h.n = 2
}

fn main() {}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:?}",
        r.diagnostics
    );
}
