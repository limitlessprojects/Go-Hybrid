//! return_single_value.rs — regression tests for #1852: each expression in a
//! positional return list is a single-value slot.

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
fn multi_value_call_in_return_list_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn pair() (int, int) {{\n    return two_ints(), 3\n}}\n\nfn main() {{\n    p := pair()\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `return two_ints(), 3`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_call_in_return_list_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn pair() (int, int) {{\n    return nothing(), 3\n}}\n\nfn main() {{\n    p := pair()\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void call in a return list, got {cs:?}"
    );
}

#[test]
fn positional_return_list_accepts() {
    let r = check(
        "fn pair() (int, int) {\n    return 1, 2\n}\n\nfn main() {\n    p := pair()\n    _ = p\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "an ordinary positional return must stay accepted, got {cs:?}"
    );
}

#[test]
fn legal_multi_value_return_accepts() {
    // `return two_ints()` as the whole list is legal Go and must not be
    // probed as a single-value slot.
    let r = check(&format!(
        "{PRELUDE}fn pair() (int, int) {{\n    return two_ints()\n}}\n\nfn main() {{\n    p := pair()\n    _ = p\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "`return two_ints()` must stay accepted, got {cs:?}"
    );
}
