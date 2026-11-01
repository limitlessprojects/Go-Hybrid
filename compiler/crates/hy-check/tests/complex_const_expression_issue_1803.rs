//! complex_const_expression_issue_1803.rs — issue #1803: Go spec §Constant
//! expressions admits `complex64(…)`, `complex128(…)`, `complex(re, im)`,
//! `real(z)`, and `imag(z)` with constant arguments in a `const`
//! initializer.  Only integer/float/string/bool conversion targets were
//! recognized, so these valid constants were falsely rejected with
//! HYB-E6003 (`hyb check` refuses an emit-buildable-Go program).

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
fn complex_type_conversions_are_constant() {
    let r = check(
        "fn main() {\n    const c1 = complex64(1.0)\n    const c2 = complex128(2.0)\n    fmt.println(c1)\n    fmt.println(c2)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "complex64/128 conversions of constants must not fire HYB-E6003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn complex_real_imag_builtins_on_constants_are_constant() {
    let r = check(
        "fn main() {\n    const c3 = complex(1.0, 2.0)\n    const c4 = real(c3)\n    const c5 = imag(c3)\n    fmt.println(c3)\n    fmt.println(c4)\n    fmt.println(c5)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "complex()/real()/imag() over constant args must not fire HYB-E6003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn complex_of_type_alias_conversions_is_constant() {
    let r = check(
        "type f64 = float64\nfn main() {\n    const c = complex64(f64(1.5))\n    fmt.println(c)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "alias-resolved conversion target inside complex() must be constant, got {:?}",
        cs
    );
}

#[test]
fn real_of_runtime_value_is_rejected() {
    let r = check(
        "fn main() {\n    x := 1.5\n    const c = real(x)\n    fmt.println(c)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "runtime argument must still fire HYB-E6003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn complex_with_runtime_arg_is_rejected() {
    let r = check(
        "fn main() {\n    x := 1.0\n    const c = complex(x, 2.0)\n    fmt.println(c)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "one runtime argument must fire HYB-E6003, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn real_wrong_arity_is_rejected() {
    let r = check("fn main() {\n    const c = real(1.0, 2.0)\n    fmt.println(c)\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "real() with two args is not a constant, got {:?}",
        cs
    );
}

#[test]
fn cap_of_constant_is_still_rejected() {
    // `cap` is only constant over an array-typed value; a literal argument
    // is not, so accepting it here would emit unbuildable Go.
    let r = check("fn main() {\n    const c = cap(3)\n    fmt.println(c)\n}\n");
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "cap() over a constant scalar must stay rejected, got {:?}",
        cs
    );
}

#[test]
fn shadowed_real_fn_is_rejected() {
    let r = check(
        "fn real(n float64) float64 {\n    return n\n}\nconst c = real(1.0)\nfn main() {\n    fmt.println(c)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "file-level fn shadows the real() builtin, got {:?}",
        cs
    );
}

#[test]
fn type_alias_named_real_is_a_conversion_target() {
    // A file type alias `real = float64` shadows the builtin *within Go*
    // too, so Go itself reads `real(1.0)` as a float64 conversion — a
    // constant.  Must stay accepted.
    let r = check(
        "type real = float64\nconst c = real(1.0)\nfn main() {\n    fmt.println(c)\n}\n",
    );
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E6003"),
        "alias-target conversion is a Go constant, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
