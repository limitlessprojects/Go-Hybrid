//! #2834 — method expressions `T.m` (Go §Method expressions): the receiver is the
//! function's first argument. Calls through `pt.get(...)` were never checked,
//! so receiver-type, arity and argument-type mismatches passed `hyb check`
//! and failed `go build`.

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

const PT: &str = "struct pt {\n    x int\n}\nfn (p pt) get() int { return p.x }\nfn (p pt) add(n int) int { return p.x + n }\n";

#[test]
fn reject_non_receiver_first_argument() {
    assert_reject(&format!("{PT}fn main() {{\n    fmt.println(pt.get(1))\n}}\n"));
}

#[test]
fn reject_missing_receiver_argument() {
    assert_reject(&format!("{PT}fn main() {{\n    fmt.println(pt.get())\n}}\n"));
}

#[test]
fn reject_too_many_arguments() {
    assert_reject(&format!(
        "{PT}fn main() {{\n    fmt.println(pt.add(pt{{x: 1}}, 2, 3))\n}}\n"
    ));
}

#[test]
fn reject_wrong_parameter_type_after_receiver() {
    assert_reject(&format!(
        "{PT}fn main() {{\n    fmt.println(pt.add(pt{{x: 1}}, \"a\"))\n}}\n"
    ));
}

#[test]
fn reject_bound_method_expression_arity() {
    assert_reject(&format!(
        "{PT}fn main() {{\n    g := pt.add\n    fmt.println(g(pt{{x: 1}}))\n}}\n"
    ));
}

#[test]
fn accept_well_formed_method_expressions() {
    assert_accept(&format!(
        "{PT}fn main() {{\n    q := pt{{x: 1}}\n    fmt.println(pt.get(q), pt.add(q, 2))\n    g := pt.add\n    fmt.println(g(q, 3))\n}}\n"
    ));
}

#[test]
fn accept_method_value_call_on_a_local() {
    // `p.add(2)` on a local is a method VALUE call with the receiver bound,
    // not a method expression: unchanged.
    assert_accept(&format!(
        "{PT}fn main() {{\n    p := pt{{x: 1}}\n    fmt.println(p.add(2))\n}}\n"
    ));
}
