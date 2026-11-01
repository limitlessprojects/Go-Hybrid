//! method_value_arity_682_u1.rs — regression for #682 U1 (method value arity).
//!
//! `f := z.add` binds a method value: the receiver is already bound, so the
//! call arity of `f` is the declaration's parameter count. The binding was
//! classified through the struct *field* table only, which has no entry for a
//! method, so `f` fell through to `OpKind::Unknown` (fail-open) and the call
//! reached emission — `hyb check` reported 0 diagnostics while `go build`
//! refused `too many arguments in call to f`.
//!
//! The rule is accept-side buildability (`rule: BUILDABLE`, HYB-E1002), so
//! these tests pin the reject side on value and pointer receivers plus the
//! accepts that must not change.

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

const VALUE_RECV: &str =
    "struct c {\n    n int\n}\n\nfn (x c) add(k int) int {\n    return x.n + k\n}\n";
const PTR_RECV: &str =
    "struct c {\n    n int\n}\n\nfn (x *c) add(k int) int {\n    return x.n + k\n}\n";
const NO_PARAM_METHOD: &str = "struct c {\n    n int\n}\n\nfn (x *c) reset() {\n    x.n = 0\n}\n";

// ---------------------------------------------------------------- reject:

#[test]
fn method_value_too_many_arguments_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
}

#[test]
fn method_value_too_few_arguments_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f()\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
}

#[test]
fn value_receiver_method_value_too_many_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        VALUE_RECV
    ));
}

#[test]
fn method_value_through_pointer_alias_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    p := &z\n    f := p.add\n    v := f(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
}

#[test]
fn method_value_aliased_then_mis_called_rejects_e1002() {
    // The count travels with the kind, so a second binding must not reopen
    // the hole: `g := f` is still the same one-parameter function value.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    g := f\n    v := g(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
}

#[test]
fn zero_parameter_method_value_called_with_argument_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.reset\n    f(1)\n    fmt.println(\"{{z.n}}\")\n}}",
        NO_PARAM_METHOD
    ));
}

// ---------------------------------------------------------------- accept:

#[test]
fn method_value_correct_arity_stays_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1)\n    fmt.println(\"{{v}}\")\n}}",
        VALUE_RECV
    ));
}

#[test]
fn zero_parameter_method_value_called_with_no_argument_stays_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.reset\n    f()\n    fmt.println(\"{{z.n}}\")\n}}",
        NO_PARAM_METHOD
    ));
}

#[test]
fn method_call_arity_control_unchanged() {
    // The direct-call path (#577 C9) must keep rejecting; the new binding
    // resolution is additive, not a replacement.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    v := z.add(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z c\n    v := z.add(1)\n    fmt.println(\"{{v}}\")\n}}",
        PTR_RECV
    ));
}

#[test]
fn struct_field_function_value_control_unchanged() {
    // A `fn`-typed *field* states its own count and was already checked; the
    // method-value path must not disturb it.
    rejects_with_e1002(
        "struct holder {\n    cb fn(a int) int\n}\n\nfn main() {\n    var h holder\n    v := h.cb(1, 2)\n    fmt.println(\"{v}\")\n}",
    );
    accepts_without_e1002(
        "struct holder {\n    cb fn(a int) int\n}\n\nfn main() {\n    var h holder\n    v := h.cb(1)\n    fmt.println(\"{v}\")\n}",
    );
}
