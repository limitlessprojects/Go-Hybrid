//! multi_value_tuple.rs — regression tests for T23 (HYB-E2002): a
//! multi-value result is not a tuple. SPEC-LANGUAGE.md §2.8 pins
//! `p := minmax(3, 9)` as a compile-fail claim; before this check the
//! program was accepted with 0 diagnostics and `go build` failed with
//! `assignment mismatch: 1 variable but minmax returns 2 values`.

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

const MINMAX: &str =
    "fn minmax(a int, b int) (int, int) {\n    if a < b { return a, b }\n    return b, a\n}\n";

fn with_main(body: &str) -> String {
    format!("{MINMAX}\nfn main() {{\n{body}}}\n")
}

#[test]
fn spec_t23_single_define_is_rejected_e2002() {
    // The exact SPEC §2.8 claim.
    let r = check(&with_main("    p := minmax(3, 9)\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn let_and_var_single_bind_are_rejected_e2002() {
    let r = check(&with_main("    let p = minmax(3, 9)\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    let r = check(&with_main(
        "    var p int = minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn single_target_assignment_is_rejected_e2002() {
    let r = check(&with_main(
        "    var p int\n    p = minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn parenthesized_call_is_still_a_multi_value() {
    let r = check(&with_main("    p := (minmax(3, 9))\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn binding_every_result_is_accepted() {
    // The fix-it shape: destructuring is the legal form (T23 "ok" line).
    let r = check(&with_main("    lo, hi := minmax(3, 9)\n    fmt.println(\"{lo} {hi}\")\n    lo, hi = minmax(hi, lo)\n    fmt.println(\"{lo} {hi}\")\n"));
    assert!(
        codes(&r).is_empty(),
        "unexpected errors: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn single_result_function_is_not_affected() {
    let r =
        check("fn one() int { return 1 }\n\nfn main() {\n    x := one()\n    fmt.println(x)\n}\n");
    assert!(
        codes(&r).is_empty(),
        "unexpected errors: {:?}",
        r.diagnostics
    );
}

#[test]
fn local_binding_shadowing_the_fn_name_stays_permissive() {
    // A local closure named like the top-level fn is not a static call —
    // its result count is not known here, so no E2002 (no false rejects).
    let r = check(&with_main(
        "    minmax := fn(a int, b int) int { return a + b }\n    p := minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert!(
        !codes(&r).iter().any(|c| c == "HYB-E2002"),
        "shadowed callee must not raise E2002: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Issue #380: HYB-E2002 (T23) bypassed through fn-value binding / foreign call
// ---------------------------------------------------------------------------

#[test]
fn fn_value_binding_single_define_is_rejected_e2002() {
    let r = check(
        "fn mm2(a int, b int) (int, int) {\n    return a + 1, b + 1\n}\n\nfn main() {\n    mm := mm2\n    p := mm(3, 9)\n    fmt.println(p)\n}\n",
    );
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn fn_value_binding_var_and_assign_rejected_e2002() {
    let r = check(
        "fn mm2(a int, b int) (int, int) {\n    return a + 1, b + 1\n}\n\nfn main() {\n    mm := mm2\n    var p int\n    p = mm(3, 9)\n    fmt.println(p)\n}\n",
    );
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);

    let r2 = check(
        "fn mm2(a int, b int) (int, int) {\n    return a + 1, b + 1\n}\n\nfn main() {\n    mm := mm2\n    var p = mm(3, 9)\n    fmt.println(p)\n}\n",
    );
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);
}

#[test]
fn declared_closure_single_define_is_rejected_e2002() {
    let r = check(
        "fn main() {\n    mm := fn(a int, b int) (int, int) { return a, b }\n    p := mm(3, 9)\n    fmt.println(p)\n}\n",
    );
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn foreign_call_single_define_is_rejected_e2002() {
    let r = check(
        "import \"strconv\"\n\nfn main() {\n    n := strconv.parse_int(\"123\", 10, 64)\n    fmt.println(n)\n}\n",
    );
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn fn_value_and_foreign_call_positive_controls_are_accepted() {
    // 1. Destructuring multi-value results from fn-value binding is accepted
    let r1 = check(
        "fn mm2(a int, b int) (int, int) {\n    return a + 1, b + 1\n}\n\nfn main() {\n    mm := mm2\n    a, b := mm(3, 9)\n    fmt.println(\"{a} {b}\")\n}\n",
    );
    assert!(codes(&r1).is_empty(), "unexpected errors: {:?}", r1.diagnostics);
    assert_eq!(r1.verdict, Verdict::Accept);

    // 2. Destructuring multi-value results from foreign call is accepted
    let r2 = check(
        "import \"strconv\"\n\nfn main() {\n    n, err := strconv.parse_int(\"123\", 10, 64)\n    fmt.println(\"{n}\")\n    _ = err\n}\n",
    );
    assert!(codes(&r2).is_empty(), "unexpected errors: {:?}", r2.diagnostics);
    assert_eq!(r2.verdict, Verdict::Accept);

    // 3. Foreign call unwrapped with ? produces 1 value and is accepted
    let r3 = check(
        "import \"strconv\"\n\nfn main() {\n    n := strconv.atoi(\"123\")?\n    fmt.println(n)\n}\n",
    );
    assert!(codes(&r3).is_empty(), "unexpected errors: {:?}", r3.diagnostics);
    assert_eq!(r3.verdict, Verdict::Accept);
}

