//! fn_call_arg_soundness.rs — regression tests for function and method call argument types:
//! verifies that passing typed int variables to float64 parameters in function and method calls
//! is rejected with HYB-E1002, while untyped integer constants remain cleanly accepted (fixes #2681).

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
fn typed_int_var_in_fn_call_to_float_param_rejected_with_e1002() {
    let src = "\
fn take_float(f float64) {
    _ = f
}

fn main() {
    var i int = 42
    take_float(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for fn call with typed int arg, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_method_call_to_float_param_rejected_with_e1002() {
    let src = "\
struct calc {}

fn (c calc) add(f float64) {
    _ = f
}

fn main() {
    var c calc
    var i int = 42
    c.add(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for method call with typed int arg, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_multi_arg_call_to_float_param_rejected_with_e1002() {
    let src = "\
fn mix(label string, f float64) {
    _ = label
    _ = f
}

fn main() {
    var i int = 42
    mix(\"test\", i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for multi-arg fn call with typed int, got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_float_params_accepted() {
    let src = "\
const file_k = 10

struct calc {}

fn (c calc) add(f float64) {
    _ = f
}

fn take_float(f float64) {
    _ = f
}

fn mix(label string, f float64) {
    _ = label
    _ = f
}

fn main() {
    const local_k = 20
    var c calc

    // Function calls with constants
    take_float(1)
    take_float(-5)
    take_float(1 + 2)
    take_float(local_k)
    take_float(file_k)

    // Method calls with constants
    c.add(42)
    c.add(local_k)
    c.add(file_k)

    // Multi-arg calls with constants
    mix(\"ok\", 100)
    mix(\"k\", local_k)

    // Normal float variable
    var f float64 = 3.14
    take_float(f)
    c.add(f)
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped integer constants passed to float params, got diags: {:?}",
        r.diagnostics
    );
}
