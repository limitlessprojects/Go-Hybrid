//! type_param_assignment_soundness.rs — regression tests for Issue #1003:
//! HYB-E1002 assignment checks when the target type is a type parameter.

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
fn assign_to_type_param_variable_rejects_e1002() {
    // Issue #1003: assigning a string to a type-parameter slot
    let src = r#"
fn put[t](slot t, v string) t {
    slot = v
    return slot
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `string` to `slot` (type `t`)")),
        "expected assignment mismatch diagnostic, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn assign_constant_to_generic_struct_field_rejects_e1002() {
    // Issue #1003: generic receiver method assigning constant 0 to field of type `t`
    let src = r#"
struct ctr[t] {
    n t
}

fn (c *ctr[t]) reset() {
    c.n = 0
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `int` to `c.n` (type `t`)")),
        "expected assignment mismatch diagnostic, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn assign_to_generic_struct_slice_element_rejects_e1002() {
    // Issue #1003: slice element store through a generic receiver
    let src = r#"
struct bag[t] {
    items []t
}

fn (b *bag[t]) fill(v string) {
    b.items[0] = v
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `string` to `b.items") && d.message.contains("(type `t`)")),
        "expected assignment mismatch diagnostic, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn assign_type_param_to_concrete_type_rejects_e1002() {
    let src = r#"
fn leak[t](val t) string {
    var s string
    s = val
    return s
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(
        r.diagnostics.iter().any(|d| d.message.contains("cannot assign a value of type `t` to `s` (type `string`)")),
        "expected assignment mismatch diagnostic, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn assign_different_type_params_rejects_e1002() {
    let src = r#"
fn swap[a, b](x a, y b) a {
    x = y
    return x
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn assign_same_type_param_stays_accepted() {
    let src = r#"
fn swap[t](a t, b t) (t, t) {
    var temp t = a
    a = b
    b = temp
    return a, b
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid type parameter assignment must accept, got: {:?}", r.diagnostics);
}

#[test]
fn generic_receiver_field_assignment_same_type_stays_accepted() {
    let src = r#"
struct pair[t] {
    first t
    second t
}

fn (p *pair[t]) copy_first() {
    p.second = p.first
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid field assignment with same type param must accept, got: {:?}", r.diagnostics);
}

#[test]
fn generic_slice_append_stays_accepted() {
    let src = r#"
struct bag[t] {
    items []t
}

fn (b *bag[t]) push(v t) {
    b.items = append(b.items, v)
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "valid generic slice append must accept, got: {:?}", r.diagnostics);
}
