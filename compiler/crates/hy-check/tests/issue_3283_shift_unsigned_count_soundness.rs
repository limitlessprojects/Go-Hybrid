//! issue_3283_shift_unsigned_count_soundness.rs — regression test for #3283:
//! shift operators (`<<`, `>>`) and compound shift assignments (`<<=`, `>>=`)
//! must reject typed signed integer shift counts with HYB-E1002 (Row 13 BUILDABLE).

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

#[test]
fn test_shift_left_signed_int_rejects_e1002() {
    let src = r#"
fn test_shl() {
    var x int = 1
    var s int = 2
    _ = x << s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for shift count of type int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_shift_right_signed_int_rejects_e1002() {
    let src = r#"
fn test_shr() {
    var x int = 16
    var s int = 2
    _ = x >> s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for shift right count of type int");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_compound_shift_left_signed_int32_rejects_e1002() {
    let src = r#"
fn test_compound_shl() {
    var x int = 1
    var s int32 = 2
    x <<= s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for compound shift count of type int32");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_compound_shift_right_signed_int64_rejects_e1002() {
    let src = r#"
fn test_compound_shr() {
    var x int64 = 64
    var s int64 = 2
    x >>= s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for compound shift count of type int64");
    let codes = error_codes(&r);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
}

#[test]
fn test_shift_signed_widths_reject_e1002() {
    for ty in &["int8", "int16", "int32", "int64"] {
        let src = format!(
            r#"
fn test_{ty}() {{
    var x int = 1
    var s {ty} = 2
    _ = x << s
}}
"#
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for shift count of type {ty}");
        let codes = error_codes(&r);
        assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002 for {ty}, got {:?}", codes);
    }
}

#[test]
fn test_shift_untyped_constant_accepts() {
    let src = r#"
fn test_untyped() {
    var x int = 1
    _ = x << 2
    _ = x >> 1
    x <<= 3
    x >>= 2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for untyped literal shift counts, got {:?}", r.diagnostics);
}

#[test]
fn test_shift_const_decl_accepts() {
    let src = r#"
const count = 2

fn test_const() {
    var x int = 1
    _ = x << count
    x <<= count
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for const shift count, got {:?}", r.diagnostics);
}

#[test]
fn test_shift_unsigned_types_accept() {
    let src = r#"
fn test_unsigned() {
    var x int = 1
    var u uint = 2
    var u8_val uint8 = 1
    var u16_val uint16 = 1
    var u32_val uint32 = 1
    var u64_val uint64 = 1
    var ptr_val uintptr = 1

    _ = x << u
    _ = x >> u8_val
    x <<= u16_val
    x >>= u32_val
    _ = x << u64_val
    _ = x << ptr_val
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for unsigned shift counts, got {:?}", r.diagnostics);
}
