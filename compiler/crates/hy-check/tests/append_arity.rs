//! append_arity.rs — regression tests for #1673: `append` requires the slice
//! plus at least one element, so `append()` and `append(s)` must reject.

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
fn append_with_one_argument_is_e2005() {
    let r = check("fn main() {\n    s := []int{1, 2}\n    x := append(s)\n    _ = x\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for `append(s)`, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn append_with_no_arguments_is_e2005() {
    let r = check("fn main() {\n    x := append()\n    _ = x\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for `append()`, got {cs:?}"
    );
}

#[test]
fn append_with_element_accepts() {
    let r = check("fn main() {\n    s := []int{1, 2}\n    x := append(s, 3)\n    _ = x\n}\n");
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2005"),
        "`append(s, 3)` must stay accepted, got {cs:?}"
    );
}

#[test]
fn append_with_spread_accepts() {
    let r = check(
        "fn main() {\n    s := []int{1, 2}\n    t := []int{3}\n    x := append(s, t...)\n    _ = x\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E2005"),
        "`append(s, t...)` must stay accepted, got {cs:?}"
    );
}
