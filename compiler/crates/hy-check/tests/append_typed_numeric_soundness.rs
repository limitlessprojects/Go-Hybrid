//! append_typed_numeric_soundness.rs — issue #2796 (Row 13 BUILDABLE).
//!
//! Go refuses built-in `append(slice, elems...)` when appended element arguments
//! have mismatched concrete numeric types:
//! - cannot use y (variable of type int) as int64 value in argument to append
//! - cannot use y (variable of type int64) as int value in argument to append
//! - cannot use f (variable of type float32) as float64 value in argument to append
//! - cannot use f (variable of type float64) as float32 value in argument to append
//!
//! Untyped numeric constants and matching types/aliases (e.g. byte -> uint8, rune -> int32) stay accepted.

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
fn append_mismatched_concrete_int_widths_rejects_e1002() {
    let cases = [
        (
            "int_var_to_int64_slice",
            "\
fn main() {
    var xs []int64 = []int64{1, 2}
    var y int = 3
    xs = append(xs, y)
    _ = xs
}
",
        ),
        (
            "int64_var_to_int_slice",
            "\
fn main() {
    var xs []int = []int{1, 2}
    var y int64 = 3
    xs = append(xs, y)
    _ = xs
}
",
        ),
        (
            "int16_var_to_int32_slice",
            "\
fn main() {
    var xs []int32 = []int32{1}
    var y int16 = 2
    xs = append(xs, y)
    _ = xs
}
",
        ),
        (
            "uint32_var_to_uint64_slice",
            "\
fn main() {
    var xs []uint64 = []uint64{1}
    var y uint32 = 2
    xs = append(xs, y)
    _ = xs
}
",
        ),
        (
            "explicit_conversion_to_int64_slice",
            "\
fn main() {
    var xs []int64 = []int64{1}
    xs = append(xs, int(3))
    _ = xs
}
",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case {name}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for case {name}, got: {codes:?}"
        );
    }
}

#[test]
fn append_mismatched_float_and_int_to_float_rejects_e1002() {
    let cases = [
        (
            "float32_var_to_float64_slice",
            "\
fn main() {
    var fs []float64 = []float64{1.0}
    var f float32 = 2.0
    fs = append(fs, f)
    _ = fs
}
",
        ),
        (
            "float64_var_to_float32_slice",
            "\
fn main() {
    var fs []float32 = []float32{1.0}
    var f float64 = 2.0
    fs = append(fs, f)
    _ = fs
}
",
        ),
        (
            "int_var_to_float64_slice",
            "\
fn main() {
    var fs []float64 = []float64{1.0}
    var i int = 2
    fs = append(fs, i)
    _ = fs
}
",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case {name}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for case {name}, got: {codes:?}"
        );
    }
}

#[test]
fn append_accepts_untyped_constants_and_aliased_types() {
    let valid_cases = [
        (
            "untyped_int_to_int64_slice",
            "\
fn main() {
    var xs []int64 = []int64{1}
    xs = append(xs, 42)
    _ = xs
}
",
        ),
        (
            "untyped_int_to_float64_slice",
            "\
fn main() {
    var fs []float64 = []float64{1.0}
    fs = append(fs, 42)
    _ = fs
}
",
        ),
        (
            "byte_var_to_uint8_slice",
            "\
fn main() {
    var bs []uint8 = []uint8{1}
    var b byte = 2
    bs = append(bs, b)
    _ = bs
}
",
        ),
        (
            "rune_var_to_int32_slice",
            "\
fn main() {
    var rs []int32 = []int32{1}
    var r rune = 2
    rs = append(rs, r)
    _ = rs
}
",
        ),
        (
            "same_int_type",
            "\
fn main() {
    var xs []int = []int{1}
    var y int = 2
    xs = append(xs, y)
    _ = xs
}
",
        ),
    ];

    for (name, src) in valid_cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid case {name}, got reject: {:?}",
            r.diagnostics
        );
    }
}
