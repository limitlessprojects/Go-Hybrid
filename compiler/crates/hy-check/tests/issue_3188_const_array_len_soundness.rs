//! issue_3188_const_array_len_soundness.rs — tests for HYB-E1002 on package-level
//! typed const declarations and HYB-E6006 on constant conversions in const and array-length positions.

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
fn const_mismatched_scalar_kind_rejects_e1002() {
    let src = "const a int = \"hello\"\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn const_concrete_scalar_width_mismatch_rejects_e1002() {
    let src = "const x int32 = int64(1)\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn const_composite_mismatch_rejects_e1002() {
    let src = "const xs []int = []string{\"a\"}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn const_invalid_type_conversion_rejects_e6006() {
    let src = "const bad = int8(2.5)\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
}

#[test]
fn array_len_invalid_type_conversion_rejects_e6006() {
    let src = "fn main() {\n    var arr [byte('Ā')]int\n    _ = arr\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
}

#[test]
fn struct_field_array_len_invalid_conv_rejects_e6006() {
    let src = "struct s_record {\n    a [int8(2.5)]int\n}\nfn main() {}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
}

#[test]
fn valid_typed_const_and_bracketed_array_accepted() {
    let src = "const n int = 42\nconst s string = \"valid\"\nconst arr [2]int = [10, 20]\nfn main() {\n    var ok [2]int\n    _ = ok\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got diagnostics: {:?}",
        r.diagnostics
    );
}
