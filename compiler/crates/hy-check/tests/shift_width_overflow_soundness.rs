//! shift_width_overflow_soundness.rs — regression tests for Issue #2714:
//! verification that constant shift count exceeding or equaling a typed integer's
//! bit width is rejected with HYB-E1002 under Row 13 BUILDABLE.

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
fn int8_and_uint8_shift_overflow_rejected() {
    // 1. int8 binary left shift >= 8
    let src1 = "fn main() {\n    var a int8 = 1\n    _ = a << 8\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. int8 binary right shift >= 8
    let src2 = "fn main() {\n    var a int8 = 1\n    _ = a >> 8\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. int8 compound left shift assign >= 8
    let src3 = "fn main() {\n    var a int8 = 1\n    a <<= 8\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));

    // 4. uint8 binary shift >= 8
    let src4 = "fn main() {\n    var b uint8 = 1\n    _ = b << 9\n}";
    let r4 = check(src4);
    assert_eq!(r4.verdict, Verdict::Reject);
    assert!(error_codes(&r4).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int16_and_uint16_shift_overflow_rejected() {
    // 1. int16 binary shift >= 16
    let src1 = "fn main() {\n    var c int16 = 1\n    _ = c << 16\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. int16 compound shift assign >= 16
    let src2 = "fn main() {\n    var c int16 = 1\n    c <<= 16\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. uint16 right shift >= 16
    let src3 = "fn main() {\n    var d uint16 = 1\n    _ = d >> 20\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int32_and_uint32_shift_overflow_rejected() {
    // 1. int32 binary shift >= 32
    let src1 = "fn main() {\n    var e int32 = 1\n    _ = e << 32\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. int32 compound shift assign >= 32
    let src2 = "fn main() {\n    var e int32 = 1\n    e <<= 32\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. uint32 right shift >= 32
    let src3 = "fn main() {\n    var f uint32 = 1\n    _ = f >> 35\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int64_and_uint64_shift_overflow_rejected() {
    // 1. int64 binary shift >= 64
    let src1 = "fn main() {\n    var g int64 = 1\n    _ = g << 64\n}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. int64 compound shift assign >= 64
    let src2 = "fn main() {\n    var g int64 = 1\n    g <<= 64\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));

    // 3. uint64 right shift >= 64
    let src3 = "fn main() {\n    var h uint64 = 1\n    _ = h >> 65\n}";
    let r3 = check(src3);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(error_codes(&r3).contains(&"HYB-E1002".to_string()));
}

#[test]
fn fn_param_and_closure_param_shift_overflow_rejected() {
    // 1. Function parameter
    let src1 = "fn shift_byte(b uint8) uint8 {\n    return b << 8\n}\nfn main() {}";
    let r1 = check(src1);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(error_codes(&r1).contains(&"HYB-E1002".to_string()));

    // 2. Closure parameter
    let src2 = "fn main() {\n    f := fn(x int16) int16 {\n        return x >> 16\n    }\n    _ = f\n}";
    let r2 = check(src2);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(error_codes(&r2).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_shifts_within_bit_width_accepted() {
    let src = r#"
fn main() {
    var a int8 = 1
    _ = a << 7

    var c int16 = 1
    _ = c << 15

    var e int32 = 1
    _ = e << 31

    var g int64 = 1
    _ = g << 63
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
