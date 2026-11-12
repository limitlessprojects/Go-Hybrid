//! iterate_single_value.rs — regression tests for #1863: the for-in range
//! expression is a single-value context.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn multi_value_range_expression_is_e1002() {
    let r = check(
        "fn two_slices() ([]int, []int) {\n    return []int{1}, []int{2}\n}\n\nfn main() {\n    for x in two_slices() {\n        _ = x\n    }\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `for x in two_slices()`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_range_expression_is_e1002() {
    let r = check(
        "fn nothing() {\n}\n\nfn main() {\n    for x in nothing() {\n        _ = x\n    }\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void range expression, got {cs:?}"
    );
}

#[test]
fn ordinary_range_accepts() {
    let r = check(
        "fn main() {\n    s := []int{1, 2}\n    for x in s {\n        _ = x\n    }\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "a plain `for x in s` must stay accepted, got {cs:?}"
    );
}
