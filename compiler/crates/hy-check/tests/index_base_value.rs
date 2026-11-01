//! index_base_value.rs — regression tests for #1854: the indexed/sliced value
//! is a single-value context.

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

const PRELUDE: &str = "fn two_slices() ([]int, []int) {\n    return []int{1}, []int{2}\n}\n\nfn nothing() {\n}\n\n";

#[test]
fn multi_value_index_base_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    x := two_slices()[0]\n    _ = x\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `two_slices()[0]`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn multi_value_slice_base_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    x := two_slices()[0:1]\n    _ = x\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `two_slices()[0:1]`, got {cs:?}"
    );
}

#[test]
fn void_index_base_is_e1002() {
    let r = check(&format!("{PRELUDE}fn main() {{\n    x := nothing()[0]\n    _ = x\n}}\n"));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void index base, got {cs:?}"
    );
}

#[test]
fn ordinary_index_and_slice_accept() {
    let r = check(
        "fn main() {\n    s := []int{1, 2, 3}\n    a := s[0]\n    b := s[0:2]\n    _ = a + len(b)\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "ordinary index/slice must stay accepted, got {cs:?}"
    );
}
