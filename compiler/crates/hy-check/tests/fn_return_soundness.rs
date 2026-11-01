//! fn_return_soundness.rs — regression tests for function return value type checking:
//! verifies that returning typed int variables from functions declaring float64 results is rejected with HYB-E1002,
//! while untyped integer constants and matching types remain accepted (fixes #2652).

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
fn typed_int_var_returned_from_float_fn_rejected_with_e1002() {
    let src = "\
fn get_val() float64 {
    var i int = 1
    return i
}

fn main() {
    _ = get_val()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn multi_value_return_float_typed_int_rejected_with_e1002() {
    let src = "\
fn get_pair() (string, float64) {
    var i int = 1
    return \"ok\", i
}

fn main() {
    _, _ = get_pair()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn untyped_int_consts_returned_from_float_fn_accepted() {
    let src = "\
const file_k = 10

fn get1() float64 {
    const local_k = 20
    return local_k
}

fn get2() float64 {
    return file_k
}

fn get3() float64 {
    return 42
}

fn get4() float64 {
    return -5
}

fn get5() float64 {
    return 1 + 2
}

fn main() {
    _ = get1()
    _ = get2()
    _ = get3()
    _ = get4()
    _ = get5()
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped constants returned from float fn, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn incompatible_type_returned_rejected_with_e1002() {
    let src = "\
fn get_str() string {
    var i int = 1
    return i
}

fn main() {
    _ = get_str()
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}
