//! generic_call_checks.rs â€” regression tests for #1961: arity and argument
//! types of an instantiated generic function are checked like any other call.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const PRELUDE: &str = "fn identity[t](x t) t {\n    return x\n}\n\nfn add[t](a t, b t) t {\n    return a\n}\n\n";

#[test]
fn generic_call_with_too_many_arguments_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    v := identity[int](10, 20, 30)\n    _ = v\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a three-argument call to a one-parameter generic, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn generic_call_with_mismatched_argument_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    v := identity[int](\"not an int\")\n    _ = v\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a string argument to identity[int], got {cs:?}"
    );
}

#[test]
fn generic_call_second_argument_mismatch_is_e1002() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    v := add[int](1, \"invalid\")\n    _ = v\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for the second add[int] argument, got {cs:?}"
    );
}

#[test]
fn well_typed_generic_call_still_accepts() {
    let r = check(&format!(
        "{PRELUDE}fn main() {{\n    a := identity[int](10)\n    b := add[int](1, 2)\n    _ = a + b\n}}\n"
    ));
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "a well-typed instantiated generic call must stay accepted, got {cs:?}"
    );
}
