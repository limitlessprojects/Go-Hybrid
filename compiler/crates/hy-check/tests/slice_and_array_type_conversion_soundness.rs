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


// Tests for #1828: constant-to-array conversions bypass check_conversion_call
#[test]
fn const_to_array_conversion_string_rejects_e6006() {
    let src = "fn main() {\n    _ = [4]byte(\"text\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert constant `\"text\"` to array type `[4]byte`")));
}

#[test]
fn const_to_array_conversion_nil_rejects_e6006() {
    let src = "fn main() {\n    _ = [4]int(nil)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert constant `nil` to array type `[4]int`")));
}

#[test]
fn const_to_array_conversion_int_rejects_e6006() {
    let src = "fn main() {\n    _ = [4]int(123)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E6006".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot convert constant `123` to array type `[4]int`")));
}

// Tests for #1827: check_call_operands omits element type compatibility for array conversions
#[test]
fn array_conversion_incompatible_slice_element_rejects_e1002() {
    let src = "fn main() {\n    s := []string{\"alpha\", \"beta\"}\n    _ = [2]int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("incompatible element types `string` and `int`")));
}

#[test]
fn array_conversion_incompatible_array_element_rejects_e1002() {
    let src = "fn main() {\n    f := [2]float64{1.0, 2.0}\n    _ = [2]int(f)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("incompatible element types `float64` and `int`")));
}

// Tests for #1825: check_append_elements spread validation
#[test]
fn append_spread_incompatible_slice_type_rejects_e1002() {
    let src = "fn main() {\n    nums := []int{1, 2}\n    strs := []string{\"a\", \"b\"}\n    _ = append(nums, strs...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use spread `strs`")));
}

#[test]
fn append_spread_string_into_int_slice_rejects_e1002() {
    let src = "fn main() {\n    nums := []int{1, 2}\n    _ = append(nums, \"hello\"...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use spread `\"hello\"`")));
}

#[test]
fn append_spread_non_slice_rejects_e1002() {
    let src = "fn main() {\n    nums := []int{1, 2}\n    x := 42\n    _ = append(nums, x...)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use spread `x`")));
}

#[test]
fn valid_append_spread_and_array_conversions_accept() {
    let src = "fn main() {\n    nums := []int{1, 2}\n    more := []int{3, 4}\n    _ = append(nums, more...)\n    bytes := []byte{1, 2}\n    _ = append(bytes, \"hello\"...)\n    s := []int{1, 2, 3}\n    _ = [3]int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
