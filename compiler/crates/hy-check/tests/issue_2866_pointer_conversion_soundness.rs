//! issue_2866_pointer_conversion_soundness.rs — regression for issue #2866 (Row 13 BUILDABLE).
//!
//! Go strictly forbids conversions between distinct pointer types unless their
//! base types have identical underlying types (or one is unsafe.Pointer).
//! Distinct numeric scalar widths (e.g. *int32 to *int64), signedness (*uint32 to *int32),
//! array lengths (*[4]int to *[8]int), and unrelated structs (*point to *node) must
//! reject with HYB-E1002. Genuine Go aliases (byte=uint8, rune=int32) and matching
//! base types stay accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn mre_rejects_distinct_scalar_and_struct_pointer_conversions() {
    let src = r#"type ptr_a = *int32
type ptr_b = *int64

struct point {
    x int64
}

struct node {
    val int64
}

type ptr_point = *point
type ptr_node = *node

fn main() {
    var a ptr_a
    var b ptr_b = ptr_b(a)

    var p ptr_point
    var n ptr_node = ptr_node(p)

    _ = b
    _ = n
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert_eq!(codes.iter().filter(|c| *c == "HYB-E1002").count(), 2);
}

#[test]
fn rejects_mismatched_scalar_pointer_conversions() {
    for (name, src) in [
        (
            "int32_ptr_to_int64_ptr",
            "type ptr_i32 = *int32\ntype ptr_i64 = *int64\nfn main() {\n    var p ptr_i32\n    _ = ptr_i64(p)\n}\n",
        ),
        (
            "addr_of_int32_to_int64_ptr",
            "type ptr_i64 = *int64\nfn main() {\n    var x int32 = 42\n    _ = ptr_i64(&x)\n}\n",
        ),
        (
            "float32_ptr_to_float64_ptr",
            "type ptr_f32 = *float32\ntype ptr_f64 = *float64\nfn main() {\n    var p ptr_f32\n    _ = ptr_f64(p)\n}\n",
        ),
        (
            "uint32_ptr_to_int32_ptr",
            "type ptr_u32 = *uint32\ntype ptr_i32 = *int32\nfn main() {\n    var p ptr_u32\n    _ = ptr_i32(p)\n}\n",
        ),
        (
            "int_ptr_to_uint_ptr",
            "type ptr_int = *int\ntype ptr_uint = *uint\nfn main() {\n    var p ptr_int\n    _ = ptr_uint(p)\n}\n",
        ),
        (
            "array_len_mismatch_pointer",
            "type ptr_arr4 = *[4]int\ntype ptr_arr8 = *[8]int\nfn main() {\n    var a ptr_arr4\n    _ = ptr_arr8(a)\n}\n",
        ),
        (
            "array_elem_mismatch_pointer",
            "type ptr_arr_i32 = *[4]int32\ntype ptr_arr_i64 = *[4]int64\nfn main() {\n    var a ptr_arr_i32\n    _ = ptr_arr_i64(a)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "case {name} should be rejected");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn accepts_matching_and_alias_pointer_conversions() {
    for (name, src) in [
        (
            "matching_int64_pointer_var",
            "type ptr_i64 = *int64\nfn main() {\n    var x int64 = 42\n    p := &x\n    _ = ptr_i64(p)\n}\n",
        ),
        (
            "matching_direct_addr_of",
            "type ptr_i64 = *int64\nfn main() {\n    var x int64 = 42\n    _ = ptr_i64(&x)\n}\n",
        ),
        (
            "byte_and_uint8_pointer_alias",
            "type ptr_u8 = *uint8\ntype ptr_byte = *byte\nfn main() {\n    var b byte = 1\n    p := &b\n    _ = ptr_u8(p)\n}\n",
        ),
        (
            "rune_and_int32_pointer_alias",
            "type ptr_i32 = *int32\ntype ptr_rune = *rune\nfn main() {\n    var r rune = 'A'\n    p := &r\n    _ = ptr_i32(p)\n}\n",
        ),
        (
            "same_pointer_type_conversion",
            "type ptr_i32 = *int32\nfn main() {\n    var a ptr_i32\n    _ = ptr_i32(a)\n}\n",
        ),
        (
            "interface_target_accepts_pointer",
            "type ptr_i32 = *int32\nfn main() {\n    var a ptr_i32\n    _ = any(a)\n}\n",
        ),
    ] {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {name} expected Accept, got {:?}",
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "case {name} expected 0 errors, got {:?}",
            r.diagnostics
        );
    }
}