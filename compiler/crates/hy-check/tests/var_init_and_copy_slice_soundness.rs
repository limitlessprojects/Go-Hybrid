//! var_init_and_copy_slice_soundness.rs — tests for HYB-E1002 rejection
//! of typed integer variables initializing float64 variable declarations,
//! and mismatched element types in built-in `copy(dst, src)`.

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
fn single_float_var_init_from_int_var_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 1
    var f float64 = i
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn let_float_var_init_from_int_var_rejects_e1002() {
    let src = "\
fn main() {
    var i int = 1
    let f float64 = i
    _ = f
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn copy_float_from_int_slice_rejects_e1002() {
    let src = "\
fn main() {
    var dst []float64
    var src []int
    copy(dst, src)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_declarations_and_copy_accepted() {
    let src = "\
fn main() {
    var f float64 = 1
    let f2 float64 = 2
    var i1 []int
    var i2 []int
    copy(i1, i2)

    var fl1 []float64
    var fl2 []float64
    copy(fl1, fl2)

    _ = f
    _ = f2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept for valid forms, got diags: {:?}", r.diagnostics);
}
