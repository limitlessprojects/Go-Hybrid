//! Regression tests for HYB-E6006 constant conversion misuse across:
//! 1. Parenthesized conversion callees (`(int)("abc")`, `(int8)(2.5)`, `([]int)("abc")`, `(u8)(300)`).
//! 2. `bool(const)` conversion targets (`bool(1)`, `bool(2.5)`, `bool("abc")`, `bool(nil)`).
//! 3. Type-aliased primitive and slice conversion targets (`type my_int = int; my_int("abc")`,
//!    `type my_slice = []int; my_slice("abc")`, `type my_elem = int; []my_elem("abc")`).
//! 4. Constant identifiers (file-level and block-scoped `const`) and constant unary/binary
//!    expressions (`const f = 2.5; int(f)`, `int(1.5 + 1.0)`, `string(1.5 + 1.0)`, `f32(1e30 * 1e20)`,
//!    `const N = 300; u8(N)`).
//! 5. File-level `const` declaration initializers (`const a = int(2.5)`, `const b = string(2.5)`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6006"),
        "expected HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e6006(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6006"),
        "must NOT fire HYB-E6006 for {:?}, got {:?}",
        src,
        cs
    );
}

#[test]
fn parenthesized_conversion_callees_reject_with_e6006() {
    rejects_with_e6006("fn main() { x := (int)(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := (int8)(2.5) _ = x }");
    rejects_with_e6006("fn main() { x := ((f32))(1e40) _ = x }");
    rejects_with_e6006("fn main() { x := (string)(2.5) _ = x }");
    rejects_with_e6006("fn main() { x := ([]int)(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := ([]byte)(1 + 2) _ = x }");
    rejects_with_e6006("fn main() { x := (u8)(300) _ = x }");
}

#[test]
fn bool_conversion_target_rejects_non_bool_constants() {
    rejects_with_e6006("fn main() { x := bool(1) _ = x }");
    rejects_with_e6006("fn main() { x := bool(2.5) _ = x }");
    rejects_with_e6006("fn main() { x := bool(\"abc\") _ = x }");
    rejects_with_e6006("fn main() { x := bool('a') _ = x }");
    rejects_with_e6006("fn main() { x := bool(nil) _ = x }");
    accepts_without_e6006("fn main() { x := bool(true) _ = x }");
    accepts_without_e6006("fn main() { x := bool(!false) _ = x }");
    accepts_without_e6006("const b = true\nfn main() { x := bool(b) _ = x }");
}

#[test]
fn type_aliased_primitive_and_slice_conversion_targets_reject() {
    rejects_with_e6006("type my_int = int\nfn main() { x := my_int(\"abc\") _ = x }");
    rejects_with_e6006("type my_i8 = i8\nfn main() { x := my_i8(2.5) _ = x }");
    rejects_with_e6006("type my_u8 = u8\nfn main() { x := my_u8(300) _ = x }");
    rejects_with_e6006("type my_f32 = f32\nfn main() { x := my_f32(1e40) _ = x }");
    rejects_with_e6006("type my_str = string\nfn main() { x := my_str(2.5) _ = x }");
    rejects_with_e6006("type my_bool = bool\nfn main() { x := my_bool(1) _ = x }");
    rejects_with_e6006("type my_slice = []int\nfn main() { x := my_slice(\"abc\") _ = x }");
    rejects_with_e6006("type my_slice = []int\nfn main() { x := my_slice(3) _ = x }");
    rejects_with_e6006("type my_elem = int\nfn main() { x := []my_elem(\"abc\") _ = x }");
    accepts_without_e6006("type my_byte = u8\nfn main() { x := []my_byte(\"abc\") _ = x }");
    accepts_without_e6006("type my_bytes = []u8\nfn main() { x := my_bytes(\"abc\") _ = x }");
}

#[test]
fn const_identifiers_and_constant_expressions_in_conversions_reject() {
    rejects_with_e6006("const f = 2.5\nfn main() { x := int(f) _ = x }");
    rejects_with_e6006("const f = 2.5\nfn main() { x := string(f) _ = x }");
    rejects_with_e6006("const s = \"abc\"\nfn main() { x := int(s) _ = x }");
    rejects_with_e6006("const s = \"abc\"\nfn main() { x := []int(s) _ = x }");
    rejects_with_e6006("const b = true\nfn main() { x := int(b) _ = x }");
    rejects_with_e6006("const b = true\nfn main() { x := string(b) _ = x }");
    rejects_with_e6006("const n = 300\nfn main() { x := u8(n) _ = x }");
    rejects_with_e6006("const f f64 = 2\nfn main() { x := string(f) _ = x }");
    rejects_with_e6006("fn main() { const f = 2.5\nx := int(f)\n_ = x }");
    rejects_with_e6006("fn main() { const s = \"abc\"\nx := []int(s)\n_ = x }");
    rejects_with_e6006("fn main() { x := int(1.5 + 1.0) _ = x }");
    rejects_with_e6006("fn main() { x := string(1.5 + 1.0) _ = x }");
    rejects_with_e6006("fn main() { x := int(\"a\" + \"b\") _ = x }");
    rejects_with_e6006("fn main() { x := []int(\"a\" + \"b\") _ = x }");
    rejects_with_e6006("fn main() { x := int(1 < 2) _ = x }");
    rejects_with_e6006("fn main() { x := f32(1e30 * 1e20) _ = x }");
    rejects_with_e6006("fn main() { x := i64(1 << 63) _ = x }");
    rejects_with_e6006("fn main() { x := u64(1 << 64) _ = x }");
}

#[test]
fn file_level_const_conversion_initializers_reject() {
    rejects_with_e6006("const a = int(2.5)\nfn main() {}");
    rejects_with_e6006("const b = string(2.5)\nfn main() {}");
    rejects_with_e6006("const c = int(\"abc\")\nfn main() {}");
    rejects_with_e6006("const d = bool(1)\nfn main() {}");
}

#[test]
fn shadowed_runtime_variables_and_legal_const_conversions_accept() {
    // A local variable `s` or `f` that shadows a file-level const is a runtime operand.
    accepts_without_e6006("const f = \"abc\"\nfn main() { f := 3\nx := int(f)\n_ = x }");
    accepts_without_e6006("const n = \"abc\"\nfn foo(n int) int { return int(n) }\nfn main() { _ = foo(1) }");
    accepts_without_e6006("const f = 2.0\nfn main() { x := int(f) _ = x }");
    accepts_without_e6006("const f f64 = 2\nfn main() { x := int(f) _ = x }");
    accepts_without_e6006("const s = \"abc\"\nfn main() { x := []byte(s) _ = x }");
    accepts_without_e6006("const c = 65\nfn main() { x := string(c) _ = x }");
}
