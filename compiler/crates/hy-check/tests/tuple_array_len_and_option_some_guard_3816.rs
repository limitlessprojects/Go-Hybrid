//! Regression tests: `tuple.rs` must check `TypeExpr::Array` length expressions
//! on local declarations and composite literals as single-value contexts, and
//! must resolve composite `option` match subjects (`opts[0]`) via
//! `composite_type_expr` when binding `some(b)` in match guards (`HYB-E2002`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_has_e2002(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.contains(&"HYB-E2002"), "expected HYB-E2002, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn array_len_type_expr_and_composite_option_match_guard_reject_e2002() {
    assert_has_e2002(
        "fn pair() (int, int) { return 1, 2 }\nfn main() {\n    var a [pair()]int = [2]int{1, 2}\n    _ = a\n}\n",
    );
    assert_has_e2002(
        "fn pair() (int, int) { return 1, 2 }\nfn main() {\n    s := [][pair()]int{}\n    _ = s\n}\n",
    );
    assert_has_e2002(
        "fn pair() (int, int) { return 1, 2 }\nfn main() {\n    m := map[[pair()]int]int{}\n    _ = m\n}\n",
    );
    assert_has_e2002(
        "struct holder { n int }\nfn (h holder) pair() (int, int) { return h.n, h.n }\nfn check_opts(opts []option[holder]) {\n    match opts[0] {\n        some(h) if h.pair() > 0 => {}\n        some(_) => {}\n        none => {}\n    }\n}\nfn main() {}\n",
    );
}

#[test]
fn single_value_composite_option_match_guard_accepts() {
    assert_accept(
        "struct holder { n int }\nfn (h holder) one() int { return h.n }\nfn check_opts(opts []option[holder]) {\n    match opts[0] {\n        some(h) if h.one() > 0 => {}\n        some(_) => {}\n        none => {}\n    }\n}\nfn main() {}\n",
    );
}
