//! fn_value_arg_types_2845.rs — issue #2845 (Row 13 BUILDABLE).
//!
//! A call through a fn-VALUED callee — a local bound to a closure, a
//! `fn(...)`-typed parameter, a method value, or a fn-typed struct field — had
//! only its ARITY checked (#577 C9, `OpKind::Fn`'s count slot). `OpKind::Fn`
//! carried the declared parameter count but not the declared parameter KINDS,
//! so a same-arity call accepted any argument kinds: `hyb check` reported 0
//! diagnostics while `go build` refused `cannot use "a" (untyped string
//! constant) as int value in argument to f`.
//!
//! The declaration's parameter kinds now ride on `OpKind::Fn`, so a same-arity
//! argument list gets the accept-side check a direct call's gets. The rule is
//! accept-side buildability (`rule: BUILDABLE`, HYB-E1002), so these tests pin
//! the reject side plus the accepts that must not change.

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
    "struct c {\n    n int\n}\n\nfn (x c) add(k int, s string) int {\n    return x.n + k\n}\n";
const HOLDER: &str = "struct holder {\n    cb fn(a int) int\n}\n";

// ------------------------------------------------------------ reject (#2845):

#[test]
fn closure_local_argument_kinds_reject_e1002() {
    // The issue's (1) and (2).
    rejects_with_e1002(
        "fn main() {\n    f := fn(x int) int {\n        return x\n    }\n    v := f(\"a\")\n    fmt.println(\"{v}\")\n}",
    );
    rejects_with_e1002(
        "fn main() {\n    h := fn(s string) int {\n        return len(s)\n    }\n    v := h(3)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn closure_local_scalar_family_mismatch_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    f := fn(b bool) int {\n        return 1\n    }\n    v := f(1)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn typed_int_into_float_parameter_of_fn_value_rejects_e1002() {
    // An untyped constant adapts; a TYPED int does not (`kinds_conflict` allows
    // `(Float, Int)` for constants, so this needs the typed-value arm).
    rejects_with_e1002(
        "fn main() {\n    f := fn(x float64) float64 {\n        return x\n    }\n    n := 3\n    v := f(n)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn fn_typed_parameter_argument_kinds_reject_e1002() {
    // The issue's (3): the fn value arrives as a `fn(...)`-typed parameter.
    // Both spellings of a parameter-list entry — the type-only `fn(int) int`
    // (the parser keeps the type in the name slot) and the named form.
    rejects_with_e1002("fn run(g fn(int) int) int {\n    return g(\"x\")\n}\n");
    rejects_with_e1002("fn run(g fn(string) int) int {\n    return g(1)\n}\n");
    rejects_with_e1002("fn run(g fn(x int) int) int {\n    return g(\"x\")\n}\n");
}

#[test]
fn top_level_fn_bound_to_a_local_rejects_e1002() {
    // `g := inc` is a fn value too, so the declared kinds must travel with it.
    rejects_with_e1002(
        "fn inc(x int) int {\n    return x\n}\n\nfn main() {\n    g := inc\n    v := g(\"a\")\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn method_value_argument_kinds_reject_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(\"a\", \"b\")\n    fmt.println(\"{{v}}\")\n}}",
        VALUE_RECV
    ));
    // The second argument only.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1, 2)\n    fmt.println(\"{{v}}\")\n}}",
        VALUE_RECV
    ));
}

#[test]
fn struct_field_fn_value_argument_kinds_reject_e1002() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    var h holder\n    v := h.cb(\"a\")\n    fmt.println(\"{{v}}\")\n}}",
        HOLDER
    ));
}

#[test]
fn aliased_fn_value_keeps_parameter_kinds() {
    rejects_with_e1002(
        "fn main() {\n    f := fn(x int) int {\n        return x\n    }\n    g := f\n    v := g(\"a\")\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn slice_element_kind_mismatch_through_fn_value_rejects_e1002() {
    rejects_with_e1002(
        "fn main() {\n    f := fn(x []int) int {\n        return 0\n    }\n    v := f([]string{\"a\"})\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn fn_value_arity_control_unchanged() {
    // The #577 C9 half must keep rejecting; the new type half is additive.
    rejects_with_e1002(
        "fn main() {\n    f := fn(x int, y int) int {\n        return x + y\n    }\n    v := f(1)\n    fmt.println(\"{v}\")\n}",
    );
}

// ------------------------------------------------------------ accept:

#[test]
fn closure_local_matching_kinds_stays_accepted() {
    accepts_without_e1002(
        "fn main() {\n    f := fn(x int) int {\n        return x\n    }\n    v := f(3)\n    fmt.println(\"{v}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    h := fn(s string) int {\n        return len(s)\n    }\n    v := h(\"a\")\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn untyped_constants_still_adapt_through_fn_values() {
    // `f(3)` for `fn(x float64)` builds in Go (`var f float64 = 3`), so the
    // untyped-constant direction must stay open.
    accepts_without_e1002(
        "fn main() {\n    f := fn(x float64) float64 {\n        return x\n    }\n    v := f(3)\n    fmt.println(\"{v}\")\n}",
    );
    accepts_without_e1002(
        "fn main() {\n    f := fn(x float64) float64 {\n        return x\n    }\n    v := f(2.5)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn pointer_parameter_with_address_of_stays_accepted() {
    accepts_without_e1002(
        "fn main() {\n    f := fn(p *int) int {\n        return 0\n    }\n    n := 1\n    v := f(&n)\n    fmt.println(\"{v}\")\n}",
    );
}

#[test]
fn fn_typed_parameter_matching_kinds_stays_accepted() {
    accepts_without_e1002("fn run(g fn(int) int) int {\n    return g(2)\n}\n");
    accepts_without_e1002("fn run(g fn(x int) int) int {\n    return g(2)\n}\n");
}

#[test]
fn method_value_and_field_matching_kinds_stay_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z c\n    f := z.add\n    v := f(1, \"a\")\n    fmt.println(\"{{v}}\")\n}}",
        VALUE_RECV
    ));
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var h holder\n    v := h.cb(1)\n    fmt.println(\"{{v}}\")\n}}",
        HOLDER
    ));
}

#[test]
fn zero_parameter_closure_call_stays_accepted() {
    accepts_without_e1002("fn main() {\n    f := fn() {\n    }\n    f()\n}");
}

#[test]
fn untyped_closure_parameter_stays_fail_open() {
    // A parameter with no type at all (`fn(x) { ... }`) carries no kind, so the
    // argument check must stay silent rather than read the name `x` as a type.
    accepts_without_e1002(
        "fn main() {\n    f := fn(x) int {\n        return 0\n    }\n    v := f(\"anything\")\n    fmt.println(\"{v}\")\n}",
    );
}

