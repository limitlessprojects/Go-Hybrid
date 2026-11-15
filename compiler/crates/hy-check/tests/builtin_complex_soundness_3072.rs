//! builtin_complex_soundness_3072.rs: regression tests for #3072, #3061, #3077.
//!
//! Under Go specification (§Complex numbers) and GUARANTEES.md Row 13 (BUILDABLE):
//! - `real()` and `imag()` require an argument of complex type and yield floating-point results.
//! - `complex()` requires two floating-point arguments and produces a complex result.
//! - `real`, `imag`, and `complex` are non-variadic built-ins and strictly reject variadic spread `...`.
//!
//! Verify that non-complex arguments to `real`/`imag`, non-float arguments to `complex`,
//! mismatched result assignments/returns, and variadic spread are rejected with HYB-E1002,
//! while valid complex operations remain accepted.

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
fn real_with_string_var_rejects() {
    rejects("fn main() {\n    var s string = \"hi\"\n    _ = real(s)\n}\n");
}

#[test]
fn imag_with_bool_var_rejects() {
    rejects("fn main() {\n    var b bool = true\n    _ = imag(b)\n}\n");
}

#[test]
fn complex_with_string_first_arg_rejects() {
    rejects("fn main() {\n    var s string = \"x\"\n    _ = complex(s, 1.0)\n}\n");
}

#[test]
fn complex_with_string_second_arg_rejects() {
    rejects("fn main() {\n    var s string = \"y\"\n    _ = complex(1.0, s)\n}\n");
}

#[test]
fn real_result_assigned_to_string_rejects() {
    rejects("fn main() {\n    c := complex(1.0, 2.0)\n    var s string\n    s = real(c)\n}\n");
}

#[test]
fn imag_result_returned_as_string_rejects() {
    rejects("fn f() string {\n    c := complex(1.0, 2.0)\n    return imag(c)\n}\nfn main() {}\n");
}

#[test]
fn real_with_variadic_spread_rejects() {
    rejects("fn main() {\n    var xs = []int{1, 2}\n    _ = real(xs...)\n}\n");
}

#[test]
fn imag_with_variadic_spread_rejects() {
    rejects("fn main() {\n    var xs = []int{1, 2}\n    _ = imag(xs...)\n}\n");
}

#[test]
fn complex_with_variadic_spread_rejects() {
    rejects("fn main() {\n    var xs = []float64{1.0, 2.0}\n    _ = complex(xs...)\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn valid_real_and_imag_float_assign_accepted() {
    accepts("fn main() {\n    c := complex(1.0, 2.0)\n    var f float64 = real(c)\n    var g float64 = imag(c)\n    _ = f\n    _ = g\n}\n");
}

#[test]
fn valid_complex_call_accepted() {
    accepts("fn main() {\n    _ = complex(1.0, 2.0)\n}\n");
}
