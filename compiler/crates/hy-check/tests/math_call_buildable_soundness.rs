//! math_call_buildable_soundness.rs — regression tests for math package member call arity and non-callable constants
//! (HYB-E1002, Row 13 BUILDABLE soundness).
//!
//! Go standard library contract (math package):
//! - Constants (Pi, E, Phi, Sqrt2, MaxInt, MaxFloat64, etc.) are package-level values, NOT functions.
//! - Functions have fixed parameter arities (NaN: 0; Sqrt, Abs, Floor, Ceil, Sin, Cos: 1; Pow, Max, Min, Atan2: 2; FMA: 3).
//! - Math functions do not accept variadic spread (`...`).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected zero errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

#[test]
fn test_math_non_callable_constants_rejected() {
    // Calling constants as functions must emit HYB-E1002
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.pi()
}"#,
    );
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.e()
}"#,
    );
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.max_int()
}"#,
    );
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.max_float64()
}"#,
    );
}

#[test]
fn test_math_arity_mismatches_rejected() {
    // nan takes 0 args
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.nan(1.0)
}"#,
    );
    // sqrt takes 1 arg
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.sqrt()
}"#,
    );
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.sqrt(1.0, 2.0)
}"#,
    );
    // pow takes 2 args
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.pow(2.0)
}"#,
    );
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.pow(2.0, 3.0, 4.0)
}"#,
    );
    // fma takes 3 args
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    _ = math.fma(1.0, 2.0)
}"#,
    );
}

#[test]
fn test_math_variadic_spread_rejected() {
    rejects_e1002(
        r#"package main
import "math"
fn main() {
    let s = []float64{1.0, 2.0}
    _ = math.max(s...)
}"#,
    );
}

#[test]
fn test_math_valid_calls_accepted() {
    accepts(
        r#"package main
import "math"
fn main() {
    _ = math.nan()
    _ = math.sqrt(4.0)
    _ = math.pow(2.0, 3.0)
    _ = math.max(1.0, 2.0)
    _ = math.fma(1.0, 2.0, 3.0)
}"#,
    );
}
