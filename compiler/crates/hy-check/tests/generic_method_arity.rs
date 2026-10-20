//! generic_method_arity.rs — regression for issue #880: `check_method_arity`
//! (the #577 C9 arity rule) looked the receiver up with `struct_name`, which
//! keeps the type arguments (`cache[int]`), while `method_decls` is keyed by
//! the base name (`cache`). The lookup missed, early-returned, and every
//! method call on every generic struct was exempt from the rule — `hyb check`
//! reported 0 diagnostics and the emitted Go failed `go build`
//! (`not enough arguments in call to c.set`).
//!
//! The rule is accept-side buildability (`rule: BUILDABLE`, HYB-E1002), so
//! these tests pin the reject side on all receiver forms plus the accepts
//! that must not change.

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

const VALUE_RECV: &str = "struct cache[k comparable] {\n    n k\n}\n\nfn (c cache[k]) set(v k) {\n    c.n = v\n}\n";
const PTR_RECV: &str = "struct cache[k comparable] {\n    n k\n}\n\nfn (c *cache[k]) set(v k) {\n    c.n = v\n}\n";
const TWO_PARAMS: &str = "struct pair2[a, b] {\n    x a\n    y b\n}\n\nfn (p pair2[a, b]) setx(v a) {\n    p.x = v\n}\n";

// ---------------------------------------------------------------- reject:

#[test]
fn generic_value_receiver_too_few_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    c.set()\n}}",
        VALUE_RECV
    ));
}

#[test]
fn generic_value_receiver_too_many_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    c.set(1, 2)\n}}",
        VALUE_RECV
    ));
}

#[test]
fn generic_pointer_receiver_too_few_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    p := &c\n    p.set()\n}}",
        PTR_RECV
    ));
}

#[test]
fn generic_two_type_parameters_too_few_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    pq := pair2[int, string]{{x: 1, y: \"s\"}}\n    pq.setx()\n}}",
        TWO_PARAMS
    ));
}

#[test]
fn generic_method_through_second_pointer_alias_rejects_e1002() {
    // The receiver kind is a pointer to a pointer after the alias: the
    // base-name fold must still resolve `cache[k]`.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    p := &c\n    q := p\n    q.set()\n}}",
        PTR_RECV
    ));
}

// ---------------------------------------------------------------- accept:

#[test]
fn generic_method_correct_arity_stays_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    c.set(3)\n    _ = c\n}}",
        VALUE_RECV
    ));
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    c := cache[int]{{n: 0}}\n    p := &c\n    p.set(3)\n    _ = c\n}}",
        PTR_RECV
    ));
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    pq := pair2[int, string]{{x: 1, y: \"s\"}}\n    pq.setx(2)\n    _ = pq\n}}",
        TWO_PARAMS
    ));
}

#[test]
fn non_generic_method_arity_control_unchanged() {
    // The rule already worked on plain structs; the fold must not weaken it.
    rejects_with_e1002(
        "struct plain {\n    x int\n}\n\nfn (p plain) set(v int) {\n    p.x = v\n}\n\nfn main() {\n    p := plain{x: 0}\n    p.set()\n}",
    );
    accepts_without_e1002(
        "struct plain {\n    x int\n}\n\nfn (p plain) set(v int) {\n    p.x = v\n}\n\nfn main() {\n    p := plain{x: 0}\n    p.set(1)\n    _ = p\n}",
    );
}

#[test]
fn generic_method_correct_arity_with_receiver_field_read_accepted() {
    // Receiver used after the call must stay accepted too (no false E1002).
    accepts_without_e1002(
        "struct cache[k comparable] {\n    n k\n}\n\nfn (c *cache[k]) set(v k) {\n    c.n = v\n}\n\nfn main() {\n    c := cache[int]{n: 0}\n    c.set(3)\n    fmt.println(\"{c.n}\")\n}",
    );
}
