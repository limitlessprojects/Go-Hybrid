//! const_overflow.rs — regression for issue #148: a constant initializer
//! outside a declared fixed-width integer type used to pass `hyb check`
//! (0 diagnostics) and then fail `go vet` / `go build` with
//! `cannot use N as T value (overflows)`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const OVERFLOW_INT8: &str = "\
fn main() {
    var n int8 = 300
    fmt.println(\"{n}\")
}
";

const OVERFLOW_UINT8: &str = "\
fn main() {
    var n uint8 = 256
    fmt.println(\"{n}\")
}
";

const OVERFLOW_BYTE_HEX: &str = "\
fn main() {
    var n byte = 0x100
    fmt.println(\"{n}\")
}
";

const OVERFLOW_NEG_INT8: &str = "\
fn main() {
    var n int8 = -129
    fmt.println(\"{n}\")
}
";

const OVERFLOW_FILE_CONST: &str = "\
const n int8 = 300

fn main() {
    fmt.println(\"{n}\")
}
";

const FITS_INT8: &str = "\
fn main() {
    var n int8 = 127
    var m int8 = -128
    fmt.println(\"{n} {m}\")
}
";

const FITS_BYTE: &str = "\
fn main() {
    var n byte = 255
    fmt.println(\"{n}\")
}
";

const VARIABLE_OPERAND_OK: &str = "\
fn main() {
    x := 300
    y := byte(x)
    fmt.println(\"{y}\")
}
";

const PAREN_OVERFLOW: &str = "\
fn main() {
    var n int8 = (300)
    fmt.println(\"{n}\")
}
";

const OVERFLOW_CHAR_INT8: &str = "\
fn main() {
    var n int8 = 'ÿ'
    fmt.println(\"{n}\")
}
";

const OVERFLOW_UNICODE_CHAR_INT8: &str = "\
fn main() {
    var n int8 = '世'
    fmt.println(\"{n}\")
}
";

const FITS_CHAR_INT8: &str = "\
fn main() {
    var n int8 = 'A'
    var r rune = 'ÿ'
    var u rune = '世'
    fmt.println(\"{n} {r} {u}\")
}
";

#[test]
fn int8_constant_300_is_rejected() {
    let r = check(OVERFLOW_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var n int8 = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_constant_underscore_3_00_is_rejected() {
    let r = check("fn main() { var n int8 = 3_00\n fmt.println(\"{n}\") }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var n int8 = 3_00, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_legacy_octal_0177_fits_in_go_must_accept() {
    let r = check("fn main() { var n int8 = 0177\n fmt.println(\"{n}\") }");
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6002"),
        "0177 is 127 in Go (fits in int8), but got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn int8_legacy_octal_0200_overflows_must_reject() {
    let r = check("fn main() { var n int8 = 0200\n fmt.println(\"{n}\") }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "0200 is 128 in Go (overflows int8), expected HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn byte_legacy_octal_0377_fits_must_accept() {
    let r = check("fn main() { var n byte = 0377\n fmt.println(\"{n}\") }");
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6002"),
        "0377 is 255 in Go (fits in byte), but got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn byte_legacy_octal_0400_overflows_must_reject() {
    let r = check("fn main() { var n byte = 0400\n fmt.println(\"{n}\") }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "0400 is 256 in Go (overflows byte), expected HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_const_legacy_octal_fits_must_accept() {
    let r = check("const max_byte byte = 0377\nfn main() { _ = max_byte }");
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6002"),
        "file-level const 0377 is 255 in Go (fits in byte), but got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn file_const_legacy_octal_overflows_must_reject() {
    let r = check("const max_byte byte = 0400\nfn main() { _ = max_byte }");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "file-level const 0400 is 256 in Go (overflows byte), expected HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn uint8_constant_256_is_rejected() {
    let r = check(OVERFLOW_UINT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn byte_hex_0x100_is_rejected() {
    let r = check(OVERFLOW_BYTE_HEX);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for byte = 0x100, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_neg_129_is_rejected() {
    let r = check(OVERFLOW_NEG_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for -129, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_level_const_overflow_is_rejected() {
    let r = check(OVERFLOW_FILE_CONST);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for file-level const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn paren_wrapped_overflow_is_rejected() {
    let r = check(PAREN_OVERFLOW);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for (300), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn int8_boundary_values_accept() {
    let r = check(FITS_INT8);
    assert!(
        error_codes(&r).is_empty(),
        "127 / -128 must fit int8, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn byte_255_accepts() {
    let r = check(FITS_BYTE);
    assert!(
        error_codes(&r).is_empty(),
        "255 must fit byte, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn variable_conversion_truncation_still_accepts() {
    // Issue #148 control: conversion from a *variable* is the runtime
    // truncation path (SPEC X4) and must stay accepted.
    let r = check(VARIABLE_OPERAND_OK);
    assert!(
        error_codes(&r).is_empty(),
        "variable conversion must stay clean, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// W10 (2026-10-17): the E6002 range check is bypassable by a file-level
// `const` alias: `const big = 300; var n int8 = big` passes `hyb check` and
// the emitted Go fails `go build` (`cannot use big (untyped int constant 300)
// as int8 value … (overflows)`). The checker must resolve `const` identifiers
// to their (constant) initializer before folding. A local binding with the
// same name shadows the file-level const and must NOT be resolved.
// ---------------------------------------------------------------------------

const CONST_ALIAS_INT8: &str = "\
const big = 300

fn main() {
    var n int8 = big
    fmt.println(\"{n}\")
}
";

const CONST_ALIAS_WIDER_OK: &str = "\
const big = 300

fn main() {
    var i int32 = big
    fmt.println(\"{i}\")
}
";

const CONST_ALIAS_SHADOWED_OK: &str = "\
const big = 300

fn main() {
    big := int8(1)
    var n int8 = big
    fmt.println(\"{n} {big}\")
}
";

#[test]
fn const_alias_overflow_is_rejected() {
    let r = check(CONST_ALIAS_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var n int8 = big (big = 300), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn const_alias_that_fits_wider_declared_type_accepts() {
    let r = check(CONST_ALIAS_WIDER_OK);
    assert!(
        error_codes(&r).is_empty(),
        "var i int32 = big (big = 300) must stay accepted, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn const_alias_shadowed_by_local_accepts() {
    // The local `big` shadows the file-level `big = 300`; the initializer
    // must not be resolved through the const table.
    let r = check(CONST_ALIAS_SHADOWED_OK);
    assert!(
        error_codes(&r).is_empty(),
        "a local shadowing the const must stay accepted, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const CONST_ALIAS_CHAIN_SHADOWED: &str = "\
const a = 300
const big = a

fn main() {
    a := int8(1)
    var n int8 = big
    fmt.println(\"{n} {a}\")
}
";

#[test]
fn const_alias_chain_with_shadowed_inner_name_is_rejected() {
    // The local `a` shadows the file-level `a` only at the use site; the
    // const's own initializer `big = a` still refers to `const a = 300`.
    let r = check(CONST_ALIAS_CHAIN_SHADOWED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for big = a = 300 used as int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn assign_overflow_int8_is_rejected() {
    let src = "\
fn main() {
    var n int8 = 0
    n = 300
    fmt.println(\"{n}\")
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for n = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn compound_assign_overflow_int8_is_rejected() {
    let src = "\
fn main() {
    var n int8 = 0
    n += 300
    fmt.println(\"{n}\")
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for n += 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn array_index_assign_overflow_is_rejected() {
    let src = "\
fn main() {
    var a [2]int8
    a[0] = 300
    fmt.println(\"{a}\")
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for a[0] = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn slice_index_assign_overflow_is_rejected() {
    let src = "\
fn main() {
    var s []int8
    s[0] = 300
    fmt.println(\"{s}\")
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for s[0] = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_value_assign_overflow_is_rejected() {
    let src = "\
fn main() {
    var m map[string]int8
    m[\"k\"] = 300
    fmt.println(\"{m}\")
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[\"k\"] = 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const OVERFLOW_STRUCT_LITERAL: &str = "\
struct user {
    age int8
}

fn main() {
    u := user{age: 300}
    fmt.println(\"{u.age}\")
}
";

const OVERFLOW_SLICE_LITERAL: &str = "\
fn main() {
    xs := []int8{300}
    fmt.println(\"{xs[0]}\")
}
";

const OVERFLOW_MAP_VALUE: &str = "\
fn main() {
    m := map[string]int8{\"a\": 300}
    v := m[\"a\"]
    fmt.println(\"{v}\")
}
";

const OVERFLOW_MAP_KEY: &str = "\
fn main() {
    m := map[int8]string{300: \"a\"}
    v := m[300]
    fmt.println(\"{v}\")
}
";

const OVERFLOW_TYPE_ALIAS_STRUCT_LITERAL: &str = "\
struct config {
    limit uint8
}

type cfg = config

fn main() {
    c := cfg{limit: 300}
    fmt.println(\"{c.limit}\")
}
";

const OVERFLOW_NESTED_COMPOSITE: &str = "\
struct item {
    code int8
}

fn main() {
    items := []item{
        item{code: 300},
    }
    fmt.println(\"{items[0].code}\")
}
";

const FITS_COMPOSITE_LITERALS: &str = "\
struct user {
    age int8
}

fn main() {
    u := user{age: 127}
    xs := []int8{127, -128}
    m := map[string]int8{\"a\": 127}
    v := m[\"a\"]
    fmt.println(\"{u.age} {xs[0]} {v}\")
}
";

#[test]
fn struct_literal_field_overflow_is_rejected() {
    let r = check(OVERFLOW_STRUCT_LITERAL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for user{{age: 300}}, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn slice_literal_element_overflow_is_rejected() {
    let r = check(OVERFLOW_SLICE_LITERAL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for []int8{{300}}, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_literal_value_overflow_is_rejected() {
    let r = check(OVERFLOW_MAP_VALUE);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for map[string]int8{{\"a\": 300}}, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_literal_key_overflow_is_rejected() {
    let r = check(OVERFLOW_MAP_KEY);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for map[int8]string{{300: \"a\"}}, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn type_alias_struct_literal_overflow_is_rejected() {
    let r = check(OVERFLOW_TYPE_ALIAS_STRUCT_LITERAL);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for aliased struct literal limit: 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_composite_literal_overflow_is_rejected() {
    let r = check(OVERFLOW_NESTED_COMPOSITE);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for nested composite literal code: 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn composite_literals_valid_range_is_accepted() {
    let r = check(FITS_COMPOSITE_LITERALS);
    assert!(
        error_codes(&r).is_empty(),
        "valid composite literals must accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const OVERFLOW_CALL_ARG: &str = "\
fn take_b(b uint8) {
    fmt.println(\"{b}\")
}
fn main() {
    take_b(300)
}
";

const OVERFLOW_CALL_ARG_NEG: &str = "\
fn take_b(b uint8) {
    fmt.println(\"{b}\")
}
fn main() {
    take_b(-1)
}
";

const FITS_CALL_ARG: &str = "\
fn take_b(b uint8) {
    fmt.println(\"{b}\")
}
fn main() {
    take_b(255)
}
";

const OVERFLOW_METHOD_ARG: &str = "\
struct user {
    age int8
}
fn (u *user) set_age(age int8) {
    u.age = age
}
fn main() {
    var u user
    u.set_age(300)
}
";

const OVERFLOW_IIFE_ARG: &str = "\
fn main() {
    fn(b uint8) {
        fmt.println(\"{b}\")
    }(300)
}
";

const OVERFLOW_APPEND_ARG: &str = "\
fn main() {
    var xs []int8
    xs = append(xs, 300)
}
";

const FITS_APPEND_ARG: &str = "\
fn main() {
    var xs []int8
    xs = append(xs, 127)
}
";

const OVERFLOW_TASK_ARG: &str = "\
fn worker(b uint8) {
    fmt.println(\"{b}\")
}
fn main() {
    task worker(300)
}
";

const OVERFLOW_ALIASED_PARAM_ARG: &str = "\
type my_byte = uint8
fn take_b(b my_byte) {
    fmt.println(\"{b}\")
}
fn main() {
    take_b(300)
}
";

#[test]
fn call_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_CALL_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "take_b(300) must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn call_argument_negative_overflow_is_rejected() {
    let r = check(OVERFLOW_CALL_ARG_NEG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "take_b(-1) for uint8 must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn call_argument_fitting_accepts() {
    let r = check(FITS_CALL_ARG);
    assert!(
        error_codes(&r).is_empty(),
        "take_b(255) must fit uint8, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn method_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_METHOD_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "u.set_age(300) must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn iife_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_IIFE_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "iife must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn append_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_APPEND_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "append(xs, 300) for []int8 must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn append_argument_fitting_accepts() {
    let r = check(FITS_APPEND_ARG);
    assert!(
        error_codes(&r).is_empty(),
        "append(xs, 127) must fit int8, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn task_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_TASK_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "task worker(300) must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn aliased_parameter_argument_overflow_is_rejected() {
    let r = check(OVERFLOW_ALIASED_PARAM_ARG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "take_b(300) for my_byte must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const OVERFLOW_RETURN: &str = "\
fn get_b() uint8 {
    return 300
}
fn main() {}
";

const OVERFLOW_RETURN_NEG: &str = "\
fn get_b() uint8 {
    return -1
}
fn main() {}
";

const FITS_RETURN: &str = "\
fn get_b() uint8 {
    return 255
}
fn main() {}
";

const OVERFLOW_MULTI_RETURN: &str = "\
fn get_pair() (int, uint8) {
    return 1, 300
}
fn main() {}
";

const OVERFLOW_CLOSURE_RETURN: &str = "\
fn main() {
    f := fn() uint8 {
        return 300
    }
    _ = f
}
";

const OVERFLOW_ALIASED_RETURN: &str = "\
type my_byte = uint8
fn get_b() my_byte {
    return 300
}
fn main() {}
";

#[test]
fn return_overflow_is_rejected() {
    let r = check(OVERFLOW_RETURN);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "return 300 for uint8 must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn return_negative_overflow_is_rejected() {
    let r = check(OVERFLOW_RETURN_NEG);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "return -1 for uint8 must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn return_fitting_accepts() {
    let r = check(FITS_RETURN);
    assert!(
        error_codes(&r).is_empty(),
        "return 255 for uint8 must fit, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn multi_return_overflow_is_rejected() {
    let r = check(OVERFLOW_MULTI_RETURN);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "return 1, 300 for (int, uint8) must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_return_overflow_is_rejected() {
    let r = check(OVERFLOW_CLOSURE_RETURN);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "closure return 300 must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn aliased_return_overflow_is_rejected() {
    let r = check(OVERFLOW_ALIASED_RETURN);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "return 300 for my_byte must emit HYB-E6002, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_int8_overflow_is_rejected() {
    let src = "
fn worker(ch chan int8) {
    ch <- 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for ch <- 300 under chan int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_uint8_negative_is_rejected() {
    let src = "
fn worker(ch chan uint8) {
    ch <- -1
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for ch <- -1 under chan uint8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_float_truncation_is_rejected() {
    let src = "
fn worker(ch chan int) {
    ch <- 2.5
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for ch <- 2.5 under chan int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_select_overflow_is_rejected() {
    let src = "
fn worker(ch chan int8) {
    select {
    ch <- 300:
        _ = 1
    default:
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for select arm ch <- 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_closure_overflow_is_rejected() {
    let src = "
fn main() {
    ch := make(chan int8, 1)
    f := fn() {
        ch <- 300
    }
    f()
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 in closure body, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_aliased_type_overflow_is_rejected() {
    let src = "
type my_byte = uint8

fn worker(ch chan my_byte) {
    ch <- 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for aliased type send, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_fitting_value_accepts() {
    let src = "
fn worker(ch chan int8) {
    ch <- 100
}
";
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "expected 100 to fit chan int8 cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn char_constant_overflow_is_rejected() {
    let r = check(OVERFLOW_CHAR_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for 'ÿ' into int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn unicode_char_constant_overflow_is_rejected() {
    let r = check(OVERFLOW_UNICODE_CHAR_INT8);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for '世' into int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn char_constant_fits_accepts() {
    let r = check(FITS_CHAR_INT8);
    assert!(
        error_codes(&r).is_empty(),
        "'A' into int8, 'ÿ' into rune, and '世' into rune must fit, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn binary_comparison_int8_overflow_rhs_rejected() {
    let src = "
fn check(x int8) bool {
    return x == 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x == 300 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_comparison_int8_overflow_lhs_rejected() {
    let src = "
fn check(x int8) bool {
    return 300 == x
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for 300 == x under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_comparison_less_than_overflow_rejected() {
    let src = "
fn check(x int8) bool {
    return x < 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x < 300 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_comparison_float_truncation_rejected() {
    let src = "
fn check(x int8) bool {
    return x == 2.5
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x == 2.5 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_arithmetic_overflow_rejected() {
    let src = "
fn add(x int8) int8 {
    return x + 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x + 300 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_arithmetic_sub_overflow_lhs_rejected() {
    let src = "
fn sub(x int8) int8 {
    return 300 - x
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for 300 - x under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_read_overflow_rejected() {
    let src = "
fn get(m map[int8]string) string {
    return m[300]
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[300] under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_write_overflow_rejected() {
    let src = "
fn set(m map[int8]string) {
    m[300] = \"hello\"
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[300] = \"hello\" under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn delete_builtin_overflow_rejected() {
    let src = "
fn del(m map[int8]string) {
    delete(m, 300)
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for delete(m, 300) under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn primitive_conversion_comparison_overflow_rejected() {
    let src = "
fn check(x int) bool {
    return int8(x) == 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for int8(x) == 300, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn comparison_and_map_fitting_values_accept() {
    let src = "
fn check(x int8, m map[int8]string) bool {
    if x == 100 {
        _ = m[100]
        delete(m, 100)
        return true
    }
    return false
}
";
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "expected valid int8 values to accept cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// PR #297 (issue #228 family D): default-int overflow at untyped var decls
// and >i128 literals failing open. Integrated on current main: parse_int_lit
// keeps main's octal/underscore handling (#518) and gains ConstInt overflow
// mapping; untyped decls check against target_int_range() via
// check_default_int_overflow (const-alias aware, matching #539's invariant).
// ---------------------------------------------------------------------------

const OVERFLOW_UNPARSED_I128_UNTYPED: &str = "\
fn main() {
    x := 999999999999999999999999999999999999999999999
    fmt.println(\"{x}\")
}
";

const OVERFLOW_I64_MAX_PLUS_ONE_UNTYPED: &str = "\
fn main() {
    x := 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_I64_MIN_MINUS_ONE_UNTYPED: &str = "\
fn main() {
    x := -9223372036854775809
    fmt.println(\"{x}\")
}
";

const OVERFLOW_VAR_UNTYPED: &str = "\
fn main() {
    var x = 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_LET_UNTYPED: &str = "\
fn main() {
    let x = 9223372036854775808
    fmt.println(\"{x}\")
}
";

const OVERFLOW_UNPARSED_I128_TYPED: &str = "\
fn main() {
    var n int8 = 999999999999999999999999999999999999999999999
    fmt.println(\"{n}\")
}
";

const CONTROLS_DEFAULT_INT_AND_CONST: &str = "\
const file_big = 9223372036854775808

fn main() {
    x := 9223372036854775807
    y := -9223372036854775808
    var u u64 = 18446744073709551615
    const local_big = 9223372036854775808
    const huge_const = 999999999999999999999999999999999999999999999
    fmt.println(\"{x} {y} {u} {local_big} {huge_const} {file_big}\")
}
";

#[test]
fn untyped_literal_past_i128_is_rejected() {
    let r = check(OVERFLOW_UNPARSED_I128_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for huge untyped int literal, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_literal_i64_max_plus_one_is_rejected() {
    let r = check(OVERFLOW_I64_MAX_PLUS_ONE_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x := 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_literal_i64_min_minus_one_is_rejected() {
    let r = check(OVERFLOW_I64_MIN_MINUS_ONE_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x := -9223372036854775809, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_var_overflow_is_rejected() {
    let r = check(OVERFLOW_VAR_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for var x = 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn untyped_let_overflow_is_rejected() {
    let r = check(OVERFLOW_LET_UNTYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for let x = 9223372036854775808, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn typed_literal_past_i128_is_rejected() {
    let r = check(OVERFLOW_UNPARSED_I128_TYPED);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for huge literal with declared type, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn controls_default_int_and_untyped_consts_accept() {
    let r = check(CONTROLS_DEFAULT_INT_AND_CONST);
    assert!(
        error_codes(&r).is_empty(),
        "controls must accept cleanly, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn untyped_constant_expression_overflow_is_rejected() {
    let r = check(
        "fn main() {\n    x := 9223372036854775807 + 1\n    fmt.println(\"{x}\")\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "default-int constant expressions must retain the main-branch ConstFold check"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn landed_constfold_regressions_remain_rejected_after_default_int_port() {
    for src in [
        "fn main() {\n    var n int8 = 100 + 100\n    fmt.println(\"{n}\")\n}\n",
        "fn main() {\n    var x int = 2.5\n    fmt.println(\"{x}\")\n}\n",
    ] {
        let r = check(src);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E6002"),
            "landed #224 overflow/truncation behavior must survive #297 integration: {:?}",
            error_codes(&r)
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn folded_default_int_shift_overflows_are_rejected() {
    for src in [
        "fn main() {\n    x := 1 << 63\n    fmt.println(\"{x}\")\n}\n",
        "fn main() {\n    x := 1 << 128\n    fmt.println(\"{x}\")\n}\n",
    ] {
        let r = check(src);
        assert!(
            error_codes(&r).iter().any(|c| c == "HYB-E6002"),
            "folded default-int shift overflow must be HYB-E6002: {:?}",
            error_codes(&r)
        );
        assert_eq!(r.verdict, Verdict::Reject);
    }
}

#[test]
fn nondecimal_underscore_literal_past_i128_is_rejected() {
    let r = check(
        "fn main() {\n    x := 0x1_0000_0000_0000_0000_0000_0000_0000_0000\n    fmt.println(\"{x}\")\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "radix parsing must retain >i128 overflow instead of failing open: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn const_alias_into_untyped_decl_is_rejected() {
    // Main invariant (W10/#539): the default-int check resolves file-level
    // const aliases like the typed check does — Go rejects
    // `const big = 9223372036854775808; x := big` with an overflow too.
    let r = check(
        "const big = 9223372036854775808\n\nfn main() {\n    x := big\n    fmt.println(\"{x}\")\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "const alias into an untyped decl must be HYB-E6002: {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// -----------------------------------------------------------------------
// Issue #576: constant overflow and float truncation in binary comparisons,
// binary arithmetic, and map indexing.
// -----------------------------------------------------------------------

#[test]
fn binary_comparison_overflow_lhs_var_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    if x == 300 {
        fmt.println(\"unreachable\")
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x == 300 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_comparison_overflow_rhs_var_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    if 300 == x {
        fmt.println(\"unreachable\")
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for 300 == x under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_comparison_float_truncation_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    if x == 2.5 {
        fmt.println(\"unreachable\")
    }
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x == 2.5 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_arithmetic_overflow_lhs_var_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    _ = x + 300
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x + 300 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_arithmetic_overflow_rhs_var_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    _ = 300 - x
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for 300 - x under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_arithmetic_float_truncation_is_rejected() {
    let src = "
fn main() {
    var x int8 = 10
    _ = x + 2.5
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for x + 2.5 under int8, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_read_overflow_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    _ = m[300]
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[300] under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_read_float_truncation_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    _ = m[2.5]
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[2.5] under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_assign_overflow_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    m[300] = \"overflow\"
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[300] = \"overflow\" under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_assign_float_truncation_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    m[2.5] = \"truncated\"
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for m[2.5] = \"truncated\" under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_delete_overflow_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    delete(m, 300)
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for delete(m, 300) under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_delete_float_truncation_is_rejected() {
    let src = "
fn main() {
    var m map[int8]string
    delete(m, 2.5)
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for delete(m, 2.5) under map[int8]string, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_index_aliased_key_overflow_is_rejected() {
    let src = "
type my_byte = uint8

fn main() {
    var m map[my_byte]string
    m[300] = \"overflow\"
}
";
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for map with aliased key my_byte, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn binary_op_and_map_fitting_values_accept() {
    let src = "
fn main() {
    var x int8 = 10
    if x == 100 {
        _ = x + 20
    }
    var m map[int8]string
    _ = m[10]
    m[10] = \"ok\"
    delete(m, 10)
}
";
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "expected fitting operations under int8 to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// Issue #998: assignment into a narrow-typed var whose RHS is a NAMED
// file-level const (`x = k`). An assign-side check existed but folded with a
// bare syntactic fold that saw `k` as NotConst; the declaration path (W10,
// `const big = 300; var n int8 = big`) resolved named consts via the file
// const map. The assign path now shares that fold, so `x = k` rejects exactly
// like the declaration twin does.
// ---------------------------------------------------------------------------

#[test]
fn assign_named_const_overflow_is_e6002() {
    let r = check(
        "const k = 70000\n\nfn main() {\n    var x int16 = 1\n    x = k\n    _ = x\n}\n",
    );
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for `x = k` with k = 70000 on int16, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn assign_named_const_wider_target_accepts() {
    // Over-reach control: 70000 fits int32, so the same named const must
    // stay accepted on a wider declared type.
    let r = check(
        "const k = 70000\n\nfn main() {\n    var y int32 = 1\n    y = k\n    _ = y\n}\n",
    );
    assert!(
        error_codes(&r).is_empty(),
        "expected 70000 into int32 to accept, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn assign_literal_overflow_still_rejected() {
    // The pre-#998 wired shape (`n = 300` on int8, literal RHS) must keep
    // rejecting — the shared fold must not weaken the literal path.
    let r = check("fn main() {\n    var n int8 = 1\n    n = 300\n    _ = n\n}\n");
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E6002"),
        "expected HYB-E6002 for `n = 300` on int8, got {:?}",
        error_codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
