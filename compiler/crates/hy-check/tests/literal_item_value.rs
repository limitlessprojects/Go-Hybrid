//! literal_item_value.rs — regression tests for #1849: every slice/map
//! composite-literal item is a single-value slot.

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

const PRELUDE: &str = "fn two_ints() (int, int) {\n    return 1, 2\n}\n\nfn nothing() {\n}\n\n";

#[test]
fn multi_value_slice_item_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    s := []int{{two_ints()}}\n    _ = s\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `[]int{{two_ints()}}`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn multi_value_map_value_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    m := map[string]int{{\"a\": two_ints()}}\n    _ = m\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a multi-value map value, got {cs:?}"
    );
}

#[test]
fn void_map_key_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    m := map[string]int{{nothing(): 1}}\n    _ = m\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void map key, got {cs:?}"
    );
}

#[test]
fn ordinary_literals_accept() {
    let r = check(
        "fn main() {\n    s := []int{1, 2, 3}\n    m := map[string]int{\"a\": 1, \"b\": 2}\n    _ = s\n    _ = m\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "ordinary composite literals must stay accepted, got {cs:?}"
    );
}
