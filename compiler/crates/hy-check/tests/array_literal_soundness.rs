//! array_literal_soundness.rs — regression tests for array composite literal element checks:
//! verifies that untyped integer constants (both literals and named constants) and any-typed elements
//! are accepted without false-positive HYB-E1002, while typed variables of incompatible types
//! continue to be properly rejected (fixes #2645).

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
fn untyped_constants_and_any_array_literals_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20
    a1 := [4]float64{1, 2.5, file_k, local_k}
    a2 := [2]float64{-5, 1 + 2}
    a_any := [3]any{1, \"text\", 3.14}
    _ = a1
    _ = a2
    _ = a_any
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped constants and any array literals, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_float_array_literal_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 1
    a := [1]float64{i}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn incompatible_string_in_int_array_literal_rejected_with_e1002() {
    let src = "\
fn main() {
    var s string = \"bad\"
    a := [1]int{s}
    _ = a
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}
