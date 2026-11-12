//! #3142 — reject method expressions on pointer-receiver methods (Go §Method expressions).
//! Under Go specification (§Method expressions, §Method sets), the method set of a defined
//! type T contains only methods declared with value receiver T. A method m declared with
//! receiver `(p *T)` is not in T's method set, and `T.m` is invalid (`needs pointer receiver (*T).m`).

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

const PT_PTR: &str = "\
struct pt {
    x int
}
fn (p *pt) set_x(v int) {
    p.x = v
}
fn (p pt) get_x() int {
    return p.x
}
";

#[test]
fn reject_pointer_receiver_method_expr_call() {
    assert_reject(&format!(
        "{PT_PTR}fn main() {{\n    p := pt{{x: 1}}\n    pt.set_x(&p, 10)\n}}\n"
    ));
}

#[test]
fn reject_pointer_receiver_method_expr_bound_value() {
    assert_reject(&format!(
        "{PT_PTR}fn main() {{\n    g := pt.set_x\n    fmt.println(g)\n}}\n"
    ));
}

#[test]
fn reject_pointer_receiver_method_expr_call_wrong_args() {
    assert_reject(&format!(
        "{PT_PTR}fn main() {{\n    pt.set_x(1, 2)\n}}\n"
    ));
}

#[test]
fn accept_valid_value_receiver_method_expr_and_direct_pointer_call() {
    assert_accept(&format!(
        "{PT_PTR}fn main() {{\n    p := pt{{x: 1}}\n    p.set_x(10)\n    fmt.println(pt.get_x(p))\n    g := pt.get_x\n    fmt.println(g(p))\n}}\n"
    ));
}
