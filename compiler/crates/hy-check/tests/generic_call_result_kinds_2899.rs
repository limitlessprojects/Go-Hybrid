//! generic_call_result_kinds_2899.rs — issue #2899 (Row 13 BUILDABLE).
//!
//! `call_result_kinds` ran `kind_of_ty` on a callee's declared result type. For
//! a generic function that result mentions the type parameter
//! (`fn id[t any](v t) t`), which resolves to `Unknown`, and every operand
//! guard treats `Unknown` as fail-open — so the call result passed the
//! var-init, argument, return, comparison and arithmetic checks while
//! `go build` refused the emitted Go.
//!
//! The type parameters are now bound (spelled out in the callee name, or
//! structurally from the arguments) and substituted into the results, with
//! fail-open kept for a parameter bound to two different kinds, bound to an
//! `Unknown` argument, or never bound.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

fn rejects_with_e1002(src: &str) {
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

fn accepts_without_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "must NOT fire HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

const FNS: &str = "fn id[t any](v t) t {\n    return v\n}\n\nfn first[t any](a []t) t {\n    return a[0]\n}\n\nfn wrap[t any](v t) []t {\n    return []t{v}\n}\n\nfn take(s string) {\n    fmt.println(s)\n}\n\n";

// ------------------------------------------------------------ reject (#2899):

#[test]
fn generic_result_in_a_mismatched_declaration_rejects_e1002() {
    // The issue's rows 1, 2 and 5.
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    var s string = id(3)\n    fmt.println(s)\n}}"
    ));
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    x := id(3)\n    var s string = x\n    fmt.println(s)\n}}"
    ));
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    var s string = id[int](3)\n    fmt.println(s)\n}}"
    ));
}

#[test]
fn generic_result_in_an_argument_or_return_rejects_e1002() {
    // The issue's rows 3 and 4.
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    take(id(3))\n}}"
    ));
    rejects_with_e1002(&format!(
        "{FNS}fn r() string {{\n    return id(3)\n}}\n\nfn main() {{\n    fmt.println(r())\n}}"
    ));
}

#[test]
fn generic_result_in_a_composite_declaration_rejects_e1002() {
    // Row 6: `wrap[t any](v t) []t`.
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    var s []string = wrap(3)\n    fmt.println(s)\n}}"
    ));
}

#[test]
fn generic_result_in_an_operand_rejects_e1002() {
    // Rows 7, 8 and 9.
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    if id(3) == \"a\" {{\n        fmt.println(\"eq\")\n    }}\n}}"
    ));
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    x := first([]int{{1}}) + \"a\"\n    fmt.println(x)\n}}"
    ));
    rejects_with_e1002(&format!(
        "{FNS}fn main() {{\n    x := first([]string{{\"a\"}})\n    y := x + 1\n    fmt.println(y)\n}}"
    ));
}

// ------------------------------------------------------------ accept:

#[test]
fn a_generic_result_in_a_matching_slot_stays_accepted() {
    accepts_without_e1002(&format!(
        "{FNS}fn main() {{\n    var n int = id(3)\n    fmt.println(n)\n}}"
    ));
    accepts_without_e1002(&format!(
        "{FNS}fn main() {{\n    var s string = id(\"a\")\n    fmt.println(s)\n}}"
    ));
    accepts_without_e1002(&format!(
        "{FNS}fn main() {{\n    take(id(\"a\"))\n}}"
    ));
    accepts_without_e1002(&format!(
        "{FNS}fn main() {{\n    var s []string = wrap(\"a\")\n    fmt.println(s)\n}}"
    ));
}

#[test]
fn an_unbindable_type_parameter_stays_fail_open() {
    // Two arguments bind `t` to two different kinds — Go resolves that call to
    // `float64`, which the kind model cannot see, so the result stays `Unknown`
    // rather than rejecting a program that builds.
    accepts_without_e1002(
        "fn pick[t any](a t, b t) t {\n    return a\n}\n\nfn main() {\n    var s string = pick(1, 2.5)\n    fmt.println(s)\n}",
    );
    // An argument the kind model cannot resolve leaves the binding unknown.
    accepts_without_e1002(
        "fn id[t any](v t) t {\n    return v\n}\n\nfn main() {\n    var x unknown_fn_result = 1\n    fmt.println(\"{x}\")\n}",
    );
}
