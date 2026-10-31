//! slice_and_array_type_conversion_soundness.rs — tests for HYB-E1002 rejection of
//! invalid slice type conversions (non-string/non-slice operands, arity mismatches,
//! and non-byte/rune conversions from non-slice types).

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
fn slice_int_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = []int(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `x` (type `int`) to `[]int`")));
}

#[test]
fn slice_string_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = []string(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `x` (type `int`) to `[]string`")));
}

#[test]
fn slice_byte_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = []byte(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `x` (type `int`) to `[]byte`")));
}

#[test]
fn slice_rune_conversion_from_bool_var_rejects_e1002() {
    let src = "fn main() {\n    b := true\n    _ = []rune(b)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `b` (type `bool`) to `[]rune`")));
}

#[test]
fn slice_int_conversion_from_string_var_rejects_e1002() {
    let src = "fn main() {\n    s := \"hello\"\n    _ = []int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `s` (type `string`) to `[]int`")));
}

#[test]
fn slice_conversion_zero_args_rejects_e1002() {
    let src = "fn main() {\n    _ = []byte()\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("conversion to `[]byte` takes exactly 1 argument, found 0")));
}

#[test]
fn slice_conversion_two_args_rejects_e1002() {
    let src = "fn main() {\n    _ = []byte(\"a\", \"b\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("conversion to `[]byte` takes exactly 1 argument, found 2")));
}

#[test]
fn slice_float_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = []float64(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `x` (type `int`) to `[]float64`")));
}

#[test]
fn slice_nested_slice_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = [][]int(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `x` (type `int`) to `[][]int`")));
}

#[test]
fn valid_byte_and_rune_slice_conversions_accept() {
    let src = "fn main() {\n    b1 := []byte(\"valid\")\n    s := \"text\"\n    b2 := []byte(s)\n    r1 := []rune(\"valid\")\n    r2 := []rune(s)\n    _ = b1\n    _ = b2\n    _ = r1\n    _ = r2\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn slice_conversion_from_incompatible_slice_element_rejects_e1002() {
    let src = "fn main() {\n    var s []string = []string{\"alpha\", \"beta\"}\n    _ = []int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `s` (type `[]string`) to `[]int`")));
}

#[test]
fn slice_byte_conversion_from_int_slice_rejects_e1002() {
    let src = "fn main() {\n    var s []int = []int{1, 2}\n    _ = []byte(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert `s` (type `[]int`) to `[]byte`")));
}

#[test]
fn slice_same_element_conversion_accepts() {
    let src = "fn main() {\n    var s []int = []int{1, 2}\n    _ = []int(s)\n    var b []byte = []byte(\"hi\")\n    _ = []uint8(b)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn slice_conversion_from_literal_incompatible_element_rejects_e1002() {
    let src = "fn main() {\n    _ = []int([]string{\"a\", \"b\"})\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

