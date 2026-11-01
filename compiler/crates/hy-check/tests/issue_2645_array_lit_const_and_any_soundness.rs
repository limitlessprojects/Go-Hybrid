//! issue_2645_array_lit_const_and_any_soundness.rs — regression tests for Issue #2645:
//! verification that untyped integer constants (including named constants) and any-typed
//! elements are accepted in array composite literals, while typed int variables remain rejected.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn named_untyped_int_constants_accepted_in_float_array_literals() {
    let src = "\
const c_val = 10

fn main() {
    const local_c = 20
    a1 := [1]float64{c_val}
    a2 := [2]float64{c_val, local_c}
    a3 := [...]float64{1, c_val, local_c}
    _ = a1
    _ = a2
    _ = a3
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for named untyped integer constants in float array literals, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn any_typed_array_literals_accept_elements_of_any_type() {
    let src = "\
fn main() {
    a1 := [1]any{1}
    a2 := [2]any{\"hello\", 42}
    a3 := [...]any{true, 3.14, \"text\"}
    _ = a1
    _ = a2
    _ = a3
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for any-typed array literals, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_variables_in_float_array_literals_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 1
    a := [1]float64{i}
    _ = a
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected reject for typed int variable in float array literal"
    );
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in diags: {:?}",
        r.diagnostics
    );
}
