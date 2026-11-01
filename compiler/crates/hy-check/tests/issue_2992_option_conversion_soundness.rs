//! issue_2992_option_conversion_soundness.rs — tests for HYB-E1002 rejection of
//! converting an option[T] value to scalar or string types (fixes #2992).

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
fn option_to_u8_conversion_rejects_e1002() {
    let src = "fn main() {\n    op := some(1)\n    _ = u8(op)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_call_result_to_float64_conversion_rejects_e1002() {
    let src = "fn f() option[int] { return some(2) }\nfn main() {\n    _ = float64(f())\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_to_string_conversion_rejects_e1002() {
    let src = "fn main() {\n    op := some(1)\n    _ = string(op)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_to_int_conversion_rejects_e1002() {
    let src = "fn main() {\n    op := some(1)\n    _ = int(op)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_to_rune_conversion_rejects_e1002() {
    let src = "fn main() {\n    op := some(1)\n    _ = rune(op)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_unwrap_to_u8_accepted() {
    let src = "fn main() {\n    op := some(1)\n    _ = u8(op.unwrap())\n}\n";
    let r = check(src);
    assert!(!error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_or_to_float64_accepted() {
    let src = "fn main() {\n    op := some(1)\n    _ = float64(op.or(3))\n}\n";
    let r = check(src);
    assert!(!error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn option_to_any_interface_accepted() {
    let src = "fn main() {\n    op := some(1)\n    _ = any(op)\n}\n";
    let r = check(src);
    assert!(!error_codes(&r).contains(&"HYB-E1002".to_string()));
}
