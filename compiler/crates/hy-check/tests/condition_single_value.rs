//! condition_single_value.rs — regression tests for #1862: a condition is a
//! single-value context, so a multi-value or void call must reject there.

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

const PRELUDE: &str = "fn two_bools() (bool, bool) {\n    return true, false\n}\n\nfn nothing() {\n}\n\n";

#[test]
fn multi_value_if_condition_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    if two_bools() {{\n    }}\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `if two_bools()`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn multi_value_for_condition_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    for two_bools() {{\n    }}\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `for two_bools()`, got {cs:?}"
    );
}

#[test]
fn void_if_condition_is_e1002() {
    let r = check(&format!("{PRELUDE}fn main() {{\n    if nothing() {{\n    }}\n}}\n"));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void condition, got {cs:?}"
    );
}

#[test]
fn boolean_conditions_still_accept() {
    let r = check(
        "fn main() {\n    x := 1\n    if x > 0 {\n    }\n    for x > 0 {\n        x = 0\n    }\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "ordinary bool conditions must stay accepted, got {cs:?}"
    );
}
