//! #2899 — the result of a call to a generic fn carries the substituted
//! type argument (inferred from the arguments or given explicitly), so
//! `var s string = id(3)` and friends are rejected instead of failing go build.

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

fn assert_reject(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

fn assert_accept(src: &str) {
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "{:?}", r.diagnostics);
}

const DECLS: &str = "fn id[t any](v t) t { return v }\nfn first[t any](a []t) t { return a[0] }\nfn wrap[t any](v t) []t { return []t{v} }\nfn pick[t any](a t, b t) t { return a }\nfn get[k comparable, v any](m map[k]v, key k) v { return m[key] }\nfn take(s string) int { return len(s) }\n";

fn prog(body: &str) -> String {
    format!("{DECLS}fn main() {{\n{body}\n}}\n")
}

#[test]
fn reject_var_init_from_inferred_generic_result() {
    assert_reject(&prog("    var s string = id(3)\n    fmt.println(s)"));
}

#[test]
fn reject_binding_from_generic_result_used_as_other_type() {
    assert_reject(&prog(
        "    x := id(3)\n    var s string = x\n    fmt.println(s)",
    ));
}

#[test]
fn reject_generic_result_as_mismatched_argument() {
    assert_reject(&prog("    fmt.println(take(id(3)))"));
}

#[test]
fn reject_generic_result_in_mismatched_return() {
    assert_reject(&format!(
        "{DECLS}fn r() string {{ return id(3) }}\nfn main() {{\n    fmt.println(r())\n}}\n"
    ));
}

#[test]
fn reject_explicit_type_argument_result() {
    assert_reject(&prog("    var s string = id[int](3)\n    fmt.println(s)"));
}

#[test]
fn reject_container_result_with_wrong_element() {
    assert_reject(&prog("    var s []string = wrap(3)\n    fmt.println(s)"));
}

#[test]
fn reject_generic_result_compared_to_other_type() {
    assert_reject(&prog(
        "    if id(3) == \"a\" {\n        fmt.println(1)\n    }",
    ));
}

#[test]
fn reject_element_result_in_mismatched_binary() {
    assert_reject(&prog(
        "    x := first([]int{1}) + \"a\"\n    fmt.println(x)",
    ));
    assert_reject(&prog(
        "    x := first([]string{\"a\"})\n    y := x + 1\n    fmt.println(y)",
    ));
}

#[test]
fn accept_well_typed_generic_results() {
    assert_accept(&prog(
        "    var s string = id(\"a\")\n    var n int = id(3)\n    var f float64 = id(2.5)\n    x := first([]string{\"a\"}) + \"b\"\n    var ws []int = wrap(1)\n    var g int = get(map[string]int{\"a\": 1}, \"a\")\n    y := id[int](4) + 1\n    fmt.println(s, n, f, x, ws, g, y, take(id(\"z\")))\n    if id(3) == 3 {\n        fmt.println(1)\n    }",
    ));
}

#[test]
fn accept_conflicting_untyped_bindings_stay_fail_open() {
    // Go infers float64 from `pick(1, 2.5)`; the kind model sees Int and
    // Float and must not guess.
    assert_accept(&prog(
        "    var p float64 = pick(1, 2.5)\n    fmt.println(p)",
    ));
}
