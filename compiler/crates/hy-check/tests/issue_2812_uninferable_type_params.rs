//! #2812 — SPEC-LANGUAGE §4 G5: a bare call to a generic fn whose type
//! parameter appears in no parameter type cannot be inferred by Go
//! (`in call to f, cannot infer t`), so hy-check must reject it.

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

const ZERO: &str = "fn zero[t any]() t {\n    var z t\n    return z\n}\n";

#[test]
fn reject_zero_arg_generic_call() {
    assert_reject(&format!(
        "{ZERO}fn main() {{\n    fmt.println(zero())\n}}\n"
    ));
}

#[test]
fn reject_result_only_param_with_typed_target() {
    assert_reject(&format!(
        "{ZERO}fn main() {{\n    var n int = zero()\n    fmt.println(n)\n}}\n"
    ));
}

#[test]
fn reject_container_result_only_param() {
    let src = "fn mk[t any]() []t {\n    return []t{}\n}\nfn main() {\n    xs := mk()\n    fmt.println(len(xs))\n}\n";
    assert_reject(src);
}

#[test]
fn reject_second_param_missing_from_args() {
    let src = "fn conv[a any, b any](x a) b {\n    var z b\n    return z\n}\nfn main() {\n    fmt.println(conv(1))\n}\n";
    assert_reject(src);
}

#[test]
fn accept_explicit_instantiation() {
    assert_accept(&format!(
        "{ZERO}fn main() {{\n    fmt.println(zero[int]())\n}}\n"
    ));
}

#[test]
fn accept_params_inferable_from_args() {
    let src = "fn pick[k comparable, v any](m map[k]v, key k) v { return m[key] }\nfn apply[a any, b any](x a, f fn(a) b) b { return f(x) }\nfn main() {\n    m := map[string]int{\"a\": 1}\n    fmt.println(pick(m, \"a\"))\n    fmt.println(apply(2, fn(x int) string { return \"s\" }))\n}\n";
    assert_accept(src);
}
