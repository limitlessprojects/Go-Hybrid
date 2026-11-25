//! any_conversion_soundness_2482.rs: regression tests for #2482.
//!
//! Under Go language rules and GUARANTEES.md Row 13 (BUILDABLE), converting an
//! interface value (such as `any`) to a concrete type requires a type assertion
//! `x.(T)` rather than a conversion call `T(x)`. Go strictly rejects
//! `int(a)`, `string(a)`, `float64(a)`, `bool(a)`, `(*int)(a)`, `uint8(a)` with:
//! "cannot convert a (variable of type any) to type ...: need type assertion".
//!
//! Because Hybrid does not provide type assertion syntax, converting `any` to
//! any concrete type can never build and must be rejected with HYB-E1002.
//! The reverse direction (`any(x)` concrete into `any`) and interface-to-interface
//! conversion (`any(a)`) must remain accepted.

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
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
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
fn any_var_converted_to_int_rejects() {
    rejects("fn main() {\n    var a any = 1\n    _ = int(a)\n}\n");
}

#[test]
fn any_var_converted_to_float64_rejects() {
    rejects("fn main() {\n    var a any = 1.0\n    _ = float64(a)\n}\n");
}

#[test]
fn any_var_converted_to_string_rejects() {
    rejects("fn main() {\n    var a any = \"hello\"\n    _ = string(a)\n}\n");
}

#[test]
fn any_var_converted_to_bool_rejects() {
    rejects("fn main() {\n    var a any = true\n    _ = bool(a)\n}\n");
}

#[test]
fn any_var_converted_to_fixed_width_int_rejects() {
    rejects("fn main() {\n    var a any = 42\n    _ = uint8(a)\n    _ = int64(a)\n}\n");
}

#[test]
fn any_param_converted_to_int_rejects() {
    rejects("fn take(a any) {\n    _ = int(a)\n}\nfn main() {\n    take(1)\n}\n");
}

#[test]
fn recover_call_converted_to_string_rejects() {
    rejects("fn main() {\n    defer fn() {\n        _ = string(recover())\n    }()\n    panic(\"boom\")\n}\n");
}

#[test]
fn recover_call_converted_to_int_rejects() {
    rejects("fn main() {\n    defer fn() {\n        _ = int(recover())\n    }()\n    panic(\"boom\")\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn concrete_converted_to_any_stays_accepted() {
    accepts("fn main() {\n    _ = any(42)\n    _ = any(\"hello\")\n    _ = any(true)\n}\n");
}

#[test]
fn any_converted_to_any_stays_accepted() {
    accepts("fn main() {\n    var a any = 1\n    _ = any(a)\n}\n");
}

#[test]
fn any_equality_and_nil_comparison_stays_accepted() {
    accepts("fn main() {\n    var a any = 10\n    if a == 10 {\n        fmt.println(\"matches\")\n    }\n    if a != nil {\n        fmt.println(\"non-nil\")\n    }\n}\n");
}
