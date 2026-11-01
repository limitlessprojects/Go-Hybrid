//! struct_lit_typed_numeric_soundness.rs — issue #2782 (Row 13 BUILDABLE).
//!
//! Go refuses struct literal field assignments with mismatched concrete numeric types:
//! - cannot use i (variable of type int) as int64 value in struct literal
//! - cannot use f (variable of type float64) as float32 value in struct literal
//! - cannot use i (variable of type int) as float32 value in struct literal
//!
//! Untyped numeric constants (literals, const expressions) and matching types/aliases
//! (e.g. byte -> uint8, rune -> int32) stay accepted.

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
fn struct_lit_typed_int_mismatch_rejects_e1002() {
    let cases = [
        (
            "int_var_to_int64_field",
            "\
struct point {
    x int64
}

fn main() {
    var i int = 42
    _ = point{ x: i }
}
",
        ),
        (
            "int_conv_to_int64_field",
            "\
struct point {
    x int64
}

fn main() {
    _ = point{ x: int(42) }
}
",
        ),
        (
            "int_var_to_int32_field",
            "\
struct point {
    x int32
}

fn main() {
    var i int = 42
    _ = point{ x: i }
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
fn struct_lit_typed_float_mismatch_rejects_e1002() {
    let cases = [
        (
            "float64_var_to_float32_field",
            "\
struct point {
    y float32
}

fn main() {
    var f float64 = 3.14
    _ = point{ y: f }
}
",
        ),
        (
            "float64_conv_to_float32_field",
            "\
struct point {
    y float32
}

fn main() {
    _ = point{ y: float64(3.14) }
}
",
        ),
        (
            "int_var_to_float32_field",
            "\
struct point {
    y float32
}

fn main() {
    var i int = 1
    _ = point{ y: i }
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
fn struct_lit_untyped_constants_and_matching_types_stay_accepted() {
    let cases = [
        (
            "untyped_int_const_to_int64",
            "\
struct point {
    x int64
}

fn main() {
    _ = point{ x: 42 }
}
",
        ),
        (
            "untyped_int_const_to_float32",
            "\
struct point {
    y float32
}

fn main() {
    _ = point{ y: 42 }
}
",
        ),
        (
            "matching_int64_var",
            "\
struct point {
    x int64
}

fn main() {
    var a int64 = 42
    _ = point{ x: a }
}
",
        ),
        (
            "matching_float32_var",
            "\
struct point {
    y float32
}

fn main() {
    var f float32 = 3.14
    _ = point{ y: f }
}
",
        ),
        (
            "byte_and_rune_aliases",
            "\
struct packet {
    data uint8
    code int32
}

fn main() {
    var b byte = 1
    var r rune = 'A'
    _ = packet{ data: b, code: r }
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
