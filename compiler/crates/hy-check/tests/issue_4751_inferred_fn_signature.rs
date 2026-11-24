//! issue_4751_inferred_fn_signature.rs — an INFERRED fn value (a fn literal
//! bound with `:=`, or a local holding one) keeps its exact signature
//! spelling, so passing/returning/assigning it where a differently-typed
//! `fn(...)` is declared must reject with HYB-E1002 rather than failing
//! `go build`. `OpKind::Fn` collapses parameter/result widths, so the kind
//! check alone cannot see `fn() int64` vs `fn() int`.
//!
//! Companion to issue_1599_fn_signature_comparison.rs, which covers the
//! declared-signature-meets-value flows; this file covers the inferred-local
//! flows added by #4751.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_4751.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected 0 errors, got {:?}\nsrc:\n{}",
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept, "src:\n{}", src);
}

#[test]
fn inferred_fn_mismatches_are_rejected() {
    // 1. Inferred fn literal passed as an argument (`go build` refuses:
    // cannot use `g` (value of type func() int64) as func() int value).
    const ARG_RESULT_WIDTH: &str = r#"
fn apply(f fn() int) int {
    return f()
}
fn main() {
    g := fn() int64 { return 1 }
    apply(g)
}
"#;
    rejects_with(ARG_RESULT_WIDTH, "HYB-E1002");

    // 2. Inferred fn literal returned from a differently-typed fn.
    const RETURN_WIDTH: &str = r#"
fn mk() fn() int {
    g := fn() int64 { return 1 }
    return g
}
fn main() {
    _ = mk()
}
"#;
    rejects_with(RETURN_WIDTH, "HYB-E1002");

    // 3. Typed declaration initialized from an inferred fn of another width.
    const VAR_DECL_WIDTH: &str = r#"
fn main() {
    g := fn() int64 { return 1 }
    var f fn() int = g
    _ = f
}
"#;
    rejects_with(VAR_DECL_WIDTH, "HYB-E1002");

    // 4. Parameter widths differ, result matches.
    const ARG_PARAM_WIDTH: &str = r#"
fn apply(f fn(int) int) int {
    return f(1)
}
fn main() {
    g := fn(s string) int { return len(s) }
    apply(g)
}
"#;
    rejects_with(ARG_PARAM_WIDTH, "HYB-E1002");
}

#[test]
fn inferred_fn_matches_are_accepted() {
    // Same signature: arg, var decl and return all stay accepted.
    const MATCHING: &str = r#"
fn apply(f fn() int) int {
    return f()
}
fn mk() fn() int {
    g := fn() int { return 1 }
    return g
}
fn main() {
    g := fn() int { return 1 }
    apply(g)
    var f fn() int = g
    _ = f
}
"#;
    accepts(MATCHING);

    // Parameter NAMES never matter, only types (`fn(a int, b string)` vs
    // declared `fn(u int, v string)` is the same signature in Go).
    const PARAM_NAMES: &str = r#"
fn main() {
    h := fn(a int, b string) bool { return len(b) == a }
    var f fn(u int, v string) bool = h
    _ = f(1, "x")
}
"#;
    accepts(PARAM_NAMES);
}
