//! Tests for generic type parameters passed to built-in functions (Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must compile under `go build`.
//! In Go, built-in functions require operands of specific core types (string, array,
//! slice, map, channel, or ordered scalars). Unconstrained generic type parameters
//! ([t any] or [t comparable]) have no core type and are rejected by Go:
//! - len(x) -> invalid argument: x (variable of type t constrained by any) for built-in len
//! - cap(x) -> invalid argument: x (variable of type t constrained by any) for built-in cap
//! - close(x) -> cannot close non-channel x (variable of type t constrained by any)
//! - delete(x, k) -> invalid argument: x (variable of type t constrained by any) is not a map
//! - clear(x) -> invalid argument: cannot clear x (variable of type t constrained by any)
//! - copy(x, y) -> invalid copy: argument must be a slice; have x (variable of type t constrained by any)
//! - append(x, 1) -> invalid argument: first argument to append must be a slice; have x (variable of type t constrained by any)
//! - min(x, y) -> invalid argument: x (variable of type t constrained by any) cannot be ordered

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_rejected(src: &str, expected_code: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == expected_code),
        "expected {expected_code} for {what}, got {errs:?}"
    );
}

fn assert_accepted(src: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for {what}, got Reject with {:#?}",
        r.diagnostics
    );
}

#[test]
fn test_generic_type_param_builtins_rejected() {
    // 1. len with [t any]
    assert_rejected(
        "fn f[t any](x t) int {\n    return len(x)\n}\n",
        "HYB-E2005",
        "len(x) with type parameter",
    );

    // 2. cap with [t any]
    assert_rejected(
        "fn f[t any](x t) int {\n    return cap(x)\n}\n",
        "HYB-E2005",
        "cap(x) with type parameter",
    );

    // 3. close with [t any]
    assert_rejected(
        "fn f[t any](x t) {\n    close(x)\n}\n",
        "HYB-E2005",
        "close(x) with type parameter",
    );

    // 4. delete with [t any] as map argument
    assert_rejected(
        "fn f[t any](x t) {\n    delete(x, 1)\n}\n",
        "HYB-E2005",
        "delete(x, 1) with type parameter map",
    );

    // 5. clear with [t any]
    assert_rejected(
        "fn f[t any](x t) {\n    clear(x)\n}\n",
        "HYB-E2005",
        "clear(x) with type parameter",
    );

    // 6. copy destination with [t any]
    assert_rejected(
        "fn f[t any](x t, ys []int) int {\n    return copy(x, ys)\n}\n",
        "HYB-E2005",
        "copy(x, ys) with type parameter dst",
    );

    // 7. copy source with [t any]
    assert_rejected(
        "fn f[t any](x t, ys []int) int {\n    return copy(ys, x)\n}\n",
        "HYB-E2005",
        "copy(ys, x) with type parameter src",
    );

    // 8. append slice with [t any]
    assert_rejected(
        "fn f[t any](x t) {\n    _ = append(x, 1)\n}\n",
        "HYB-E2005",
        "append(x, 1) with type parameter slice",
    );

    // 9. min with [t any]
    assert_rejected(
        "fn f[t any](x t, y t) t {\n    return min(x, y)\n}\n",
        "HYB-E2005",
        "min(x, y) with type parameter",
    );

    // 10. max with [t any]
    assert_rejected(
        "fn f[t any](x t, y t) t {\n    return max(x, y)\n}\n",
        "HYB-E2005",
        "max(x, y) with type parameter",
    );

    // 11. inferred local variable binding y := x
    assert_rejected(
        "fn f[t any](x t) int {\n    y := x\n    return len(y)\n}\n",
        "HYB-E2005",
        "len(y) with inferred type parameter local",
    );

    // 12. comparable constraint [t comparable]
    assert_rejected(
        "fn f[t comparable](x t) int {\n    return len(x)\n}\n",
        "HYB-E2005",
        "len(x) with comparable type parameter",
    );

    // 13. struct field of generic type
    assert_rejected(
        "struct container[t any] {\n    val t\n}\nfn f[t any](c container[t]) int {\n    return len(c.val)\n}\n",
        "HYB-E2005",
        "len(c.val) with struct field type parameter",
    );
}

#[test]
fn test_generic_type_param_builtins_controls_accepted() {
    // Slices of type parameter
    assert_accepted(
        "fn f[t any](xs []t) int {\n    return len(xs)\n}\n",
        "len(xs) with []t",
    );
    assert_accepted(
        "fn f[t any](xs []t) int {\n    return cap(xs)\n}\n",
        "cap(xs) with []t",
    );
    assert_accepted(
        "fn f[t any](xs []t) {\n    clear(xs)\n}\n",
        "clear(xs) with []t",
    );
    assert_accepted(
        "fn f[t any](xs []t, x t) []t {\n    return append(xs, x)\n}\n",
        "append(xs, x) with []t and element t",
    );
    assert_accepted(
        "fn f[t any](dst []t, src []t) int {\n    return copy(dst, src)\n}\n",
        "copy(dst, src) with []t",
    );

    // Channel of type parameter
    assert_accepted(
        "fn f[t any](ch chan t) {\n    close(ch)\n}\n",
        "close(ch) with chan t",
    );

    // Maps with type parameter
    assert_accepted(
        "fn f[t any](m map[string]t) {\n    delete(m, \"foo\")\n}\n",
        "delete(m, key) with map[string]t",
    );
    assert_accepted(
        "fn f[t comparable](m map[t]int, k t) {\n    delete(m, k)\n}\n",
        "delete(m, k) with map[t]int and key t",
    );
    assert_accepted(
        "fn f[t any](m map[string]t) {\n    clear(m)\n}\n",
        "clear(m) with map[string]t",
    );
}
