//! #3082 — a generic call's arguments were never kind-checked. Each reject case
//! is a row of the issue's table (accepted on main, `go build` refuses it); the
//! accept half pins Go's call-site inference so the fix cannot overshoot.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn run(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn rejects(src: &str) -> bool {
    let r = run(src);
    r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

const EQ: &str = "fn eq[t comparable](a t, b t) bool { return a == b }\n";
const SAME: &str = "fn same[t any](a t, b t) t { return a }\n";
const PUSH: &str = "fn push[t any](xs []t, v t) []t { return append(xs, v) }\n";
const BOTH: &str = "fn both[t any](a []t, b []t) int { return len(a) + len(b) }\n";
const PUT: &str = "fn put[k comparable, v any](m map[k]v, key k, val v) {\n    m[key] = val\n}\n";
const ID: &str = "fn id[t any](a t) t { return a }\n";
const PAIR: &str = "fn pair[a any, b any](x a, y b) a { return x }\n";

fn with(decl: &str, body: &str) -> String {
    format!("{decl}fn main() {{\n{body}\n}}\n")
}

#[test]
fn argument_conflicting_with_a_fixed_type_parameter_is_rejected() {
    // rows 1-3, 5: argument vs argument
    assert!(rejects(&with(EQ, "    _ = eq(1, \"a\")")));
    assert!(rejects(&with(SAME, "    x := 1\n    y := \"a\"\n    _ = same(x, y)")));
    assert!(rejects(&with(SAME, "    _ = same(true, 1)")));
    assert!(rejects(&with(
        PUSH,
        "    s := \"a\"\n    _ = push([]int{1}, s)"
    )));
    // row 6: the same parameter reached through a composite on both sides
    assert!(rejects(&with(BOTH, "    _ = both([]int{1}, []string{\"a\"})")));
}

#[test]
fn composite_and_map_parameters_bind_their_element_and_key_types() {
    // rows 4, 7, 8
    assert!(rejects(&with(PUSH, "    fmt.println(push([]int{1}, \"a\"))")));
    assert!(rejects(&with(
        PUT,
        "    m := map[string]int{}\n    put(m, \"a\", \"b\")"
    )));
    assert!(rejects(&with(PUT, "    m := map[string]int{}\n    put(m, 1, 2)")));
}

#[test]
fn explicit_type_arguments_bind_the_parameters() {
    // rows 9-10
    assert!(rejects(&with(ID, "    fmt.println(id[int](\"x\"))")));
    assert!(rejects(&with(PAIR, "    _ = pair[int, string](1, 2)")));
}

#[test]
fn go_call_site_inference_controls_stay_accepted() {
    // untyped numeric constants unify to the wider type, and a matching call
    // keeps passing: the fix must not overshoot
    let cases = [
        with(SAME, "    fmt.println(same(1, 2.5))"),
        with(SAME, "    fmt.println(same(\"a\", \"b\"))"),
        with(PUSH, "    fmt.println(push([]float64{1.5}, 2))"),
        with(PUSH, "    fmt.println(push([]int{1}, 2))"),
        with(EQ, "    fmt.println(eq(1, 2))"),
        with(ID, "    fmt.println(id[int](3))"),
        with(PAIR, "    fmt.println(pair[int, string](1, \"a\"))"),
        with(PUT, "    m := map[string]int{}\n    put(m, \"a\", 1)"),
    ];
    for src in cases {
        let r = run(&src);
        assert_eq!(r.verdict, Verdict::Accept, "{} -> {:?}", src, r.diagnostics);
    }
}
