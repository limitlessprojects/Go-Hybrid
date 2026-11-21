//! complex_scalars_operand_soundness.rs: regression tests for #2896 (Row 13 BUILDABLE).
//!
//! In Go (§Complex numbers) and GUARANTEES.md Row 13 (BUILDABLE):
//! - `complex(r, i)` requires both operands to be floating-point numbers of identical type (`float32` or `float64`).
//! - `complex(r, i)` rejects typed integers (`int32`, `int64`, etc.) and mismatched float widths (`float32` vs `float64`).
//! - `real(c)` and `imag(c)` require the operand to be a complex number (`complex64` or `complex128`).
//! - `real(c)` and `imag(c)` reject non-complex numbers (`float32`, `float64`, `int32`, `int`, etc.).

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

fn rejects(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002" || c == "HYB-E2005"),
        "expected error diagnostic for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
}

// ---------------------------------------------------------------- reject:

#[test]
fn complex_mismatched_float_widths_rejects() {
    rejects("fn main() {\n    var r float32 = 1.0\n    var i float64 = 2.0\n    _ = complex(r, i)\n}\n");
}

#[test]
fn complex_typed_int_args_rejects() {
    rejects("fn main() {\n    _ = complex(int32(1), int32(2))\n}\n");
}

#[test]
fn complex_typed_int64_args_rejects() {
    rejects("fn main() {\n    var r int64 = 1\n    var i int64 = 2\n    _ = complex(r, i)\n}\n");
}

#[test]
fn real_typed_float64_arg_rejects() {
    rejects("fn main() {\n    _ = real(float64(1.0))\n}\n");
}

#[test]
fn real_typed_float32_var_rejects() {
    rejects("fn main() {\n    var x float32 = 1.0\n    _ = real(x)\n}\n");
}

#[test]
fn imag_typed_float64_arg_rejects() {
    rejects("fn main() {\n    _ = imag(float64(1.0))\n}\n");
}

#[test]
fn imag_typed_float32_var_rejects() {
    rejects("fn main() {\n    var x float32 = 1.0\n    _ = imag(x)\n}\n");
}

#[test]
fn real_typed_int_arg_rejects() {
    rejects("fn main() {\n    _ = real(int32(1))\n}\n");
}

#[test]
fn imag_typed_int_arg_rejects() {
    rejects("fn main() {\n    _ = imag(int32(1))\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn complex_matching_float32_accepted() {
    accepts("fn main() {\n    var r float32 = 1.0\n    var i float32 = 2.0\n    c := complex(r, i)\n    _ = c\n}\n");
}

#[test]
fn complex_matching_float64_accepted() {
    accepts("fn main() {\n    var r float64 = 1.0\n    var i float64 = 2.0\n    c := complex(r, i)\n    _ = c\n}\n");
}

#[test]
fn real_and_imag_on_complex64_accepted() {
    accepts("fn main() {\n    var r float32 = 1.0\n    var i float32 = 2.0\n    c := complex(r, i)\n    var re float32 = real(c)\n    var im float32 = imag(c)\n    _ = re\n    _ = im\n}\n");
}

#[test]
fn real_and_imag_on_complex128_accepted() {
    accepts("fn main() {\n    var r float64 = 1.0\n    var i float64 = 2.0\n    c := complex(r, i)\n    var re float64 = real(c)\n    var im float64 = imag(c)\n    _ = re\n    _ = im\n}\n");
}
