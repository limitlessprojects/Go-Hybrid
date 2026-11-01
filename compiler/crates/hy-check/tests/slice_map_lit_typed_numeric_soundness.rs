//! slice_map_lit_typed_numeric_soundness.rs — issue #2788 (Row 13 BUILDABLE).
//!
//! Go refuses slice, array, and map composite literals with mismatched concrete numeric types:
//! - cannot use i (variable of type int) as int64 value in array or slice literal
//! - cannot use f (variable of type float64) as float32 value in array or slice literal
//! - cannot use i (variable of type int) as int64 value in map literal
//! - cannot use f (variable of type float64) as float32 value in map literal
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
fn slice_and_array_lit_typed_numeric_mismatch_rejects_e1002() {
    let cases = [
        (
            "int_var_to_int64_slice",
            "\
fn main() {
    var i int = 42
    _ = []int64{ i }
}
",
        ),
        (
            "int_conv_to_int64_slice",
            "\
fn main() {
    _ = []int64{ int(42) }
}
",
        ),
        (
            "float64_var_to_float32_slice",
            "\
fn main() {
    var f float64 = 3.14
    _ = []float32{ f }
}
",
        ),
        (
            "int_var_to_int64_array",
            "\
fn main() {
    var i int = 42
    _ = [1]int64{ i }
}
",
        ),
        (
            "float64_var_to_float32_array",
            "\
fn main() {
    var f float64 = 3.14
    _ = [1]float32{ f }
}
",
        ),
    ];

    for (name, src) in cases {
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
fn map_lit_typed_numeric_mismatch_rejects_e1002() {
    let cases = [
        (
            "int_var_to_int64_map_key",
            "\
fn main() {
    var i int = 42
    _ = map[int64]string{ i: \"val\" }
}
",
        ),
        (
            "int_var_to_int64_map_val",
            "\
fn main() {
    var i int = 42
    _ = map[string]int64{ \"key\": i }
}
",
        ),
        (
            "float64_var_to_float32_map_key",
            "\
fn main() {
    var f float64 = 3.14
    _ = map[float32]string{ f: \"val\" }
}
",
        ),
        (
            "float64_var_to_float32_map_val",
            "\
fn main() {
    var f float64 = 3.14
    _ = map[string]float32{ \"key\": f }
}
",
        ),
    ];

    for (name, src) in cases {
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
fn slice_and_map_untyped_constants_and_matching_types_stay_accepted() {
    let cases = [
        (
            "untyped_int_const_to_int64_slice",
            "\
fn main() {
    _ = []int64{ 42 }
}
",
        ),
        (
            "untyped_float_const_to_float32_slice",
            "\
fn main() {
    _ = []float32{ 3.14 }
}
",
        ),
        (
            "matching_int64_var_in_slice",
            "\
fn main() {
    var a int64 = 42
    _ = []int64{ a }
}
",
        ),
        (
            "matching_float32_var_in_slice",
            "\
fn main() {
    var f float32 = 3.14
    _ = []float32{ f }
}
",
        ),
        (
            "byte_and_rune_aliases_in_slice",
            "\
fn main() {
    var b byte = 1
    var r rune = 'A'
    _ = []uint8{ b }
    _ = []int32{ r }
}
",
        ),
        (
            "matching_types_in_map",
            "\
fn main() {
    var k int64 = 1
    var v float32 = 2.0
    _ = map[int64]float32{ k: v }
}
",
        ),
        (
            "untyped_constants_in_map",
            "\
fn main() {
    _ = map[int64]float32{ 1: 2.0 }
}
",
        ),
    ];

    for (name, src) in cases {
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
