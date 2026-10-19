//! #729 — SPEC G3 constraints on generic functions.
//!
//! `hyb check` accepted operators on type-parameter operands under ANY
//! constraint and accepted a non-comparable type argument for a
//! `comparable` parameter; both die at `go build` with
//! `type parameter t cannot use operator <` and
//! `[]int does not satisfy comparable`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {diags:?}"));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn ordering_on_any_type_parameter_rejects() {
    // The issue's first reproducer: `less(1, 2)` builds and runs, but the
    // generic BODY must not compile under `any` — no ordering operator is
    // ever defined on a type parameter in the MVP constraint set.
    let r = check(
        "fn less[t any](a t, b t) bool {\n\
         \treturn a < b\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(less(1, 2))\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `<` on a type parameter, got {cs:?}"
    );
    assert!(
        r.diagnostics.iter().any(|d| {
            d.message.contains("type parameter `t`") && d.message.contains("constraint `any`")
        }),
        "message should name the type parameter and its constraint: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn arithmetic_on_any_type_parameter_rejects() {
    let r = check(
        "fn plus[t any](a t, b t) t {\n\
         \treturn a + b\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(plus(1, 2))\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `+` on a type parameter, got {cs:?}"
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("not defined on type parameter `t`")),
        "message should name the operator's target: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn equality_on_any_type_parameter_rejects() {
    // The issue title's third body shape: `==` under `any` — `any`'s type set
    // contains non-comparable types, so Go refuses the comparison.
    let r = check(
        "fn eq_any[t any](a t, b t) bool {\n\
         \treturn a == b\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(eq_any(1, 2))\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `==` under `any`, got {cs:?}"
    );
    assert!(
        r.diagnostics.iter().any(|d| {
            d.message
                .contains("`==` requires a comparable type, but type parameter `t` has constraint `any`")
        }),
        "message should name the missing comparable constraint: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn not_on_any_type_parameter_rejects() {
    let r = check(
        "fn neg[t any](a t) bool {\n\
         \treturn !a\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(neg(true))\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for `!` on a type parameter, got {cs:?}"
    );
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains("unary operator `!` is not defined on type parameter `t`")),
        "message should name the unary operator: {:?}",
        r.diagnostics
    );
}

#[test]
fn equality_on_comparable_type_parameter_accepts() {
    // The control for the `==` rule: `comparable` provides `==` / `!=`, so
    // the identical body must stay legal.
    let r = check(
        "fn eq_cmp[t comparable](a t, b t) bool {\n\
         \treturn a == b\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(eq_cmp(1, 2))\n\
         }\n",
    );
    assert!(
        r.diagnostics.is_empty(),
        "`==` must stay legal under `comparable`: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn non_comparable_inferred_type_argument_rejects() {
    // The issue's second reproducer: `xs` is `[]int`, the parameter demands
    // `comparable`, and Go rejects the call at instantiation.
    let r = check(
        "fn same[t comparable](a t, b t) {}\n\
         \n\
         fn main() {\n\
         \txs := []int{1}\n\
         \tsame(xs, xs)\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for a non-comparable type argument, got {cs:?}"
    );
    assert!(
        r.diagnostics.iter().any(|d| {
            d.message
                .contains("type argument `[]int` does not satisfy constraint `comparable`")
        }),
        "message should name the type argument and the constraint: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn explicit_non_comparable_type_argument_rejects() {
    // Same rule with the type argument written instead of inferred (G5's
    // explicit-instantiation path).
    let r = check(
        "fn same[t comparable](a t, b t) {}\n\
         \n\
         fn main() {\n\
         \txs := []int{1}\n\
         \tsame[[]int](xs, xs)\n\
         }\n",
    );
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for an explicit non-comparable type argument, got {cs:?}"
    );
    assert!(
        r.diagnostics.iter().any(|d| {
            d.message
                .contains("type argument `[]int` does not satisfy constraint `comparable`")
        }),
        "message should name the explicit type argument: {:?}",
        r.diagnostics
    );
}

#[test]
fn comparable_type_arguments_still_accept() {
    // Concrete primitives are comparable: the call-site check must not become
    // a blanket rejection of generic calls.
    let r = check(
        "fn same[t comparable](a t, b t) {}\n\
         \n\
         fn main() {\n\
         \tsame(1, 2)\n\
         \tsame(\"a\", \"b\")\n\
         }\n",
    );
    assert!(
        r.diagnostics.is_empty(),
        "comparable type arguments must accept: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn generic_identity_and_concrete_operators_still_accept() {
    // Regression pins: the canonical pre-existing generic shape, the concrete
    // operator path, and an omitted-constraint declaration (parsed as `any`)
    // must all behave exactly as they did before the guard existed.
    let ok = check(
        "fn id[t any](x t) t {\n\
         \treturn x\n\
         }\n\
         \n\
         fn less(a int, b int) bool {\n\
         \treturn a < b\n\
         }\n\
         \n\
         fn bare[t](x t) t {\n\
         \treturn x\n\
         }\n\
         \n\
         fn main() {\n\
         \tfmt.println(id(5), less(1, 2), bare(\"s\"))\n\
         }\n",
    );
    assert!(
        ok.diagnostics.is_empty(),
        "existing generic shapes and concrete operators must not regress: {:?}",
        ok.diagnostics
    );
    assert_eq!(ok.verdict, Verdict::Accept);
}
