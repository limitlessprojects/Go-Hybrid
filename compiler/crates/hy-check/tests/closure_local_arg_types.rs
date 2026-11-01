//! Calls through a fn-valued local or parameter (`f := fn(x int) int {...};
//! f("a")`, `fn run(g fn(int) int) { g("x") }`) had their arity checked
//! (#577 C9) but never their argument types, so a mismatched argument passed
//! `hyb check` and failed `go build` (`cannot use "a" (untyped string
//! constant) as int value in argument to f`).

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

#[test]
fn reject_string_arg_to_closure_local_int_param() {
    assert_reject(
        "fn main() {\n    f := fn(x int) int { return x }\n    fmt.println(f(\"a\"))\n}\n",
    );
}

#[test]
fn reject_int_arg_to_closure_local_string_param() {
    assert_reject(
        "fn main() {\n    h := fn(s string) int { return len(s) }\n    fmt.println(h(3))\n}\n",
    );
}

#[test]
fn reject_mismatch_through_fn_typed_parameter() {
    assert_reject(
        "fn run(g fn(int) int) int { return g(\"x\") }\nfn main() {\n    fmt.println(run(fn(x int) int { return x }))\n}\n",
    );
}

#[test]
fn reject_second_argument_mismatch() {
    assert_reject(
        "fn main() {\n    f := fn(a int, b string) int { return a + len(b) }\n    fmt.println(f(1, 2))\n}\n",
    );
}

#[test]
fn accept_matching_closure_local_call() {
    assert_accept(
        "fn main() {\n    f := fn(x int, s string) int { return x + len(s) }\n    fmt.println(f(1, \"ab\"))\n}\n",
    );
}

#[test]
fn accept_matching_fn_typed_parameter_call() {
    assert_accept(
        "fn run(g fn(int) int) int { return g(2) }\nfn main() {\n    fmt.println(run(fn(x int) int { return x }))\n}\n",
    );
}

#[test]
fn accept_shadowed_binding_uses_inner_signature() {
    assert_accept(
        "fn main() {\n    f := fn(x int) int { return x }\n    fmt.println(f(1))\n    if true {\n        f := fn(s string) int { return len(s) }\n        fmt.println(f(\"a\"))\n    }\n}\n",
    );
}
