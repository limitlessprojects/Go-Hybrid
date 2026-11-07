//! type_conversion_operand_soundness.rs — tests for HYB-E1002 rejection of
//! invalid type conversions (converting non-structs to structs, bool to/from other types,
//! string to/from non-convertibles, and void calls as conversion operands).

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
fn struct_conversion_from_int_literal_rejects_e1002() {
    let src = "struct holder {\n    n int\n}\nfn main() {\n    _ = holder(1)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn struct_conversion_from_int_var_rejects_e1002() {
    let src = "struct holder {\n    n int\n}\nfn main() {\n    x := 1\n    _ = holder(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn struct_conversion_from_string_rejects_e1002() {
    let src = "struct holder {\n    s string\n}\nfn main() {\n    _ = holder(\"bad\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn bool_conversion_from_int_literal_rejects_e1002() {
    let src = "fn main() {\n    _ = bool(1)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn bool_conversion_from_int_var_rejects_e1002() {
    let src = "fn main() {\n    x := 1\n    _ = bool(x)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_conversion_from_bool_var_rejects_e1002() {
    let src = "fn main() {\n    b := true\n    _ = int(b)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_conversion_from_bool_var_rejects_e1002() {
    let src = "fn main() {\n    b := true\n    _ = string(b)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_conversion_from_string_var_rejects_e1002() {
    let src = "fn main() {\n    s := \"abc\"\n    _ = int(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn string_conversion_from_float_var_rejects_e1002() {
    let src = "fn main() {\n    f := 1.5\n    _ = string(f)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn conversion_from_void_call_rejects_e1002() {
    let src = "fn noop() {}\nfn main() {\n    _ = int(noop())\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_numeric_conversions_accept() {
    let src = "fn main() {\n    fl := 1.5\n    i := int(fl)\n    f := float64(1)\n    _ = f\n    _ = i\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn valid_string_from_int_accepts() {
    let src = "fn main() {\n    s := string(65)\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn valid_slice_conversions_accept() {
    let src = "fn main() {\n    b := []byte(\"hello\")\n    s := string(b)\n    _ = s\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn valid_interface_conversion_accepts() {
    let src = "fn main() {\n    a := any(42)\n    _ = a\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}

#[test]
fn slice_conversion_with_incompatible_elements_rejects_e1002() {
    let src = "type my_slice = []string\nfn main() {\n    var s []int\n    _ = my_slice(s)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn map_conversion_with_incompatible_elements_rejects_e1002() {
    let src = "type my_map = map[string]int\nfn main() {\n    var m map[int]string\n    _ = my_map(m)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn chan_conversion_with_incompatible_elements_rejects_e1002() {
    let src = "type my_chan = chan string\nfn main() {\n    var c chan int\n    _ = my_chan(c)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

