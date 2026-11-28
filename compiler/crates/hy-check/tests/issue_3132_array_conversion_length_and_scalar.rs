//! issue_3132_array_conversion_length_and_scalar.rs — issue #3132 (Row 13 BUILDABLE).
//!
//! Under Go specification (§Conversions) and Row 13 BUILDABLE:
//! - Conversions between array types require identical underlying types: both array length
//!   and element type must match.
//! - Conversions from slice to array (`[N]T(s)`) require identical element types.
//!
//! Reject with HYB-E1002 when:
//! 1. Array length mismatches (e.g. `[3]int` to `[4]int`).
//! 2. Array element scalar mismatches (e.g. `[4]int32` to `[4]int64`).
//! 3. Slice-to-array element scalar mismatches (e.g. `[]int32` to `[4]int64`).
//!
//! Accept controls:
//! - Valid array-to-array with identical length and element type.
//! - Valid slice-to-array with identical element type.

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
fn array_conversion_length_mismatch_rejects_e1002() {
    let cases = [
        (
            "named array length mismatch",
            "type a_arr = [3]int\ntype b_arr = [4]int\nfn test(a a_arr) {\n    _ = b_arr(a)\n}\n",
        ),
        (
            "direct array length mismatch in param",
            "type b_arr = [4]int\nfn test(a [3]int) {\n    _ = b_arr(a)\n}\n",
        ),
        (
            "literal array conversion length mismatch",
            "type b_arr = [4]int\nfn test() {\n    _ = b_arr([3]int{1, 2, 3})\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn array_conversion_scalar_mismatch_rejects_e1002() {
    let cases = [
        (
            "array to array int32 to int64 mismatch",
            "type a_arr = [4]int32\ntype b_arr = [4]int64\nfn test(a a_arr) {\n    _ = b_arr(a)\n}\n",
        ),
        (
            "array to array float32 to float64 mismatch",
            "type a_arr = [4]float32\ntype b_arr = [4]float64\nfn test(a a_arr) {\n    _ = b_arr(a)\n}\n",
        ),
        (
            "slice to array int32 to int64 mismatch",
            "type s_t = []int32\ntype b_arr = [4]int64\nfn test(s s_t) {\n    _ = b_arr(s)\n}\n",
        ),
        (
            "slice literal to array int32 to int64 mismatch",
            "type b_arr = [4]int64\nfn test() {\n    _ = b_arr([]int32{1, 2, 3, 4})\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for {name}");
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for {name}, got: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_array_and_slice_conversions_accept() {
    let cases = [
        (
            "same array type conversion",
            "type a4 = [4]int\nfn test(a a4) {\n    _ = a4(a)\n}\n",
        ),
        (
            "slice to array matching int element",
            "type s_t = []int\ntype a4 = [4]int\nfn test(s s_t) {\n    _ = a4(s)\n}\n",
        ),
        (
            "slice literal to array matching int element",
            "type a4 = [4]int\nfn test() {\n    _ = a4([]int{1, 2, 3, 4})\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for {name}, got diagnostics: {:?}",
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "unexpected error codes for {name}: {:?}",
            r.diagnostics
        );
    }
}
