//! try_single_value.rs — regression tests for #1960: `f()?` must be classified
//! by the value shape of the call it unwraps, minus the error result.

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

const PRELUDE: &str = "fn run() error {\n    return nil\n}\n\nfn multi() (int, int, error) {\n    return 1, 2, nil\n}\n\n";

#[test]
fn void_try_as_unary_operand_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() error {{\n    x := -run()?\n    _ = x\n    return nil\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `-run()?` (void in value position), got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn void_try_as_slice_bound_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() error {{\n    s := []int{{1, 2, 3}}\n    sub := s[run()?:2]\n    _ = sub\n    return nil\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a void try used as a slice bound, got {cs:?}"
    );
}

#[test]
fn multi_value_try_as_index_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() error {{\n    s := []int{{1, 2, 3}}\n    idx := s[multi()?]\n    _ = idx\n    return nil\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a two-value try used as an index, got {cs:?}"
    );
}

#[test]
fn single_value_try_still_binds() {
    // `(int, error)?` is exactly one value; the guard must not reject it.
    let r = check(
        "fn one() (int, error) {\n    return 1, nil\n}\n\nfn main() error {\n    x := one()?\n    _ = x\n    return nil\n}\n",
    );
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "`x := one()?` is a valid single-value binding, got {cs:?}"
    );
}
