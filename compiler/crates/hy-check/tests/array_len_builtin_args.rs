//! Regression for issue #2102: builtin calls inside array length expressions
//! (`[len(1)]int`, `[cap(hi) + 1]int`) bypassed `check_expr_builtin_args`, so
//! they passed `hyb check` with 0 diagnostics and the emitted Go failed
//! `go build` (Row 13 BUILDABLE soundness violation).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_has(r: &hy_check::CheckResult, code: &str) {
    let cs = error_codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}",
        code,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn len_in_array_len_rejected() {
    let r = check("fn main() {\n    var a [len(1)]int\n    _ = a\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn cap_in_array_len_rejected() {
    let r = check(
        "fn main() {\n    hi := \"x\"\n    b := [cap(hi) + 1]int{0}\n    _ = b\n}\n",
    );
    assert_has(&r, "HYB-E2005");
}

#[test]
fn len_in_type_alias_array_len_rejected() {
    let r = check("type arr_alias = [len()]int\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn min_in_struct_field_array_len_rejected() {
    let r = check("struct box_shaped {\n    a [min()]int\n}\n");
    assert_has(&r, "HYB-E2005");
}

#[test]
fn valid_const_array_len_still_accepted() {
    let r = check("fn main() {\n    a := [3]int{0, 1, 2}\n    _ = a\n}\n");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn valid_len_of_array_in_array_len_accepted() {
    // `len` of an actual array is a valid builtin call; the array-length site
    // must only reject INVALID builtin calls, not every builtin.
    let r = check(
        "fn main() {\n    xs := [2]int{1, 2}\n    a := [len(xs)]int{0}\n    _ = a\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
