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


// Tests for #1838: check_call_operands slice conversion element compatibility
#[test]
fn slice_conversion_incompatible_element_type_rejects_e1002() {
    let src = "fn main() {\n    s := []string{\"alpha\", \"beta\"}\n    _ = []int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("incompatible element types `string` and `int`")));
}

#[test]
fn slice_conversion_int_to_byte_slice_rejects_e1002() {
    let src = "fn main() {\n    s := []int{1, 2}\n    _ = []byte(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// Tests for #1840: conversion_kinds_conflict recursive container compatibility
#[test]
fn named_slice_conversion_incompatible_element_rejects_e1002() {
    let src = "type MySlice []string\nfn main() {\n    s := []int{1, 2, 3}\n    _ = MySlice(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn named_map_conversion_incompatible_key_val_rejects_e1002() {
    let src = "type MyMap map[string]int\nfn main() {\n    var m map[int]string\n    _ = MyMap(m)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn named_chan_conversion_incompatible_elem_rejects_e1002() {
    let src = "type MyChan chan string\nfn main() {\n    var c chan int\n    _ = MyChan(c)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// Tests for #1839: copy(dst, str) accepts only byte slices
#[test]
fn copy_string_into_int_slice_rejects_e1002() {
    let src = "fn main() {\n    var dst []int = []int{1, 2, 3, 4}\n    var src string = \"test\"\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot copy string into slice with element type `int`")));
}

#[test]
fn copy_string_into_byte_slice_accepts() {
    let src = "fn main() {\n    var dst []byte = []byte{1, 2, 3, 4}\n    var src string = \"test\"\n    copy(dst, src)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got: {:?}", r.diagnostics);
}
