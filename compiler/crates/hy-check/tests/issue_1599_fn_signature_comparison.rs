//! issue_1599_fn_signature_comparison.rs — function types must be compared
//! by signature (parameter and result types): fn values with mismatched
//! parameter/result types must reject with HYB-E1002 rather than failing `go build`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_fn_sig.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
fn fn_signature_mismatches_are_rejected() {
    // 1. Minimal repro from issue #1599: var init with different param type
    const REPRO_VAR_PARAM: &str = r#"
fn g(s string) int {
    return len(s)
}
fn main() {
    var f fn(int) int = g
    _ = f
}
"#;
    rejects_with(REPRO_VAR_PARAM, "HYB-E1002");

    // 2. Closure literal with different param type
    const REPRO_CLOSURE_PARAM: &str = r#"
fn main() {
    var f fn(int) = fn(s string) {}
    _ = f
}
"#;
    rejects_with(REPRO_CLOSURE_PARAM, "HYB-E1002");

    // 3. Closure literal with different result type
    const REPRO_CLOSURE_RESULT: &str = r#"
fn main() {
    var f fn() int = fn() string { return "a" }
    _ = f
}
"#;
    rejects_with(REPRO_CLOSURE_RESULT, "HYB-E1002");

    // 4. Function with float result assigned to int-result fn variable
    const REPRO_RESULT_FLOAT: &str = r#"
fn g(n int) float64 {
    return 1.0
}
fn main() {
    var f fn(int) int = g
    _ = f
}
"#;
    rejects_with(REPRO_RESULT_FLOAT, "HYB-E1002");

    // 5. Function with void result assigned to int-result fn variable
    const REPRO_RESULT_VOID: &str = r#"
fn g(n int) {
}
fn main() {
    var f fn(int) int = g
    _ = f
}
"#;
    rejects_with(REPRO_RESULT_VOID, "HYB-E1002");

    // 6. Struct literal field with mismatched fn type
    const REPRO_STRUCT_FIELD: &str = r#"
struct runner {
    cb fn(int) int
}
fn g(s string) int {
    return len(s)
}
fn main() {
    r := runner{cb: g}
    _ = r
}
"#;
    rejects_with(REPRO_STRUCT_FIELD, "HYB-E1002");

    // 7. Return statement with mismatched fn type
    const REPRO_RETURN_FN: &str = r#"
fn g(s string) int {
    return len(s)
}
fn mk() fn(int) int {
    return g
}
"#;
    rejects_with(REPRO_RETURN_FN, "HYB-E1002");

    // 8. Assignment statement with mismatched result arity
    const REPRO_ASSIGN_ARITY: &str = r#"
fn minmax(a int, b int) (int, int) {
    return a, b
}
fn plus(a int, b int) int {
    return a + b
}
fn main() {
    mm := minmax
    mm = plus
    _ = mm
}
"#;
    rejects_with(REPRO_ASSIGN_ARITY, "HYB-E1002");
}

#[test]
fn fn_signature_matches_are_accepted() {
    const VALID_VAR_INIT: &str = r#"
fn g(x int) int {
    return x * 2
}
fn main() {
    var f fn(int) int = g
    _ = f(2)
}
"#;
    accepts(VALID_VAR_INIT);

    const VALID_NAMED_PARAMS: &str = r#"
fn g(x int, s string) bool {
    return len(s) == x
}
fn main() {
    var f fn(a int, b string) bool = g
    _ = f(1, "a")
}
"#;
    accepts(VALID_NAMED_PARAMS);

    const VALID_CLOSURE: &str = r#"
fn main() {
    var f fn() int = fn() int { return 42 }
    _ = f()
}
"#;
    accepts(VALID_CLOSURE);
}
