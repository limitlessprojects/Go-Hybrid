//! fn_type_scalar_widths.rs — issue #4751: function-type compatibility must
//! compare scalar widths, not just kinds. `fn(int) int` must not accept
//! `fn(int64) int` (or `fn(float64) float32`, ...): the kinds match, so the
//! pre-#4751 checker passed the assignment and `go build` failed with
//! `cannot use g (variable of type func(int64) int) as func(int) int value`.
//! A width-only mismatch must reject with HYB-E1002 at check time.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_fn_width.hy";
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
fn fn_scalar_width_mismatches_are_rejected() {
    // 1. Issue repro row 1: var init, param width int64 vs int.
    const REPRO_VAR_PARAM: &str = r#"
fn g(x int64) int {
    return 1
}
fn main() {
    var f fn(int) int = g
    _ = f
}
"#;
    rejects_with(REPRO_VAR_PARAM, "HYB-E1002");
    // The diagnostic must name the width, not print the same `fn(int) int`
    // on both sides (the pre-#4751 label could not tell them apart).
    let r = check(REPRO_VAR_PARAM);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("int64")),
        "expected the diagnostic to name int64, got {:?}",
        r.diagnostics
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
    );

    // 2. Issue repro row 2: var init, float param width float64 vs float32.
    const REPRO_VAR_FLOAT_PARAM: &str = r#"
fn g(x float64) int {
    return 1
}
fn main() {
    var f fn(float32) int = g
    _ = f
}
"#;
    rejects_with(REPRO_VAR_FLOAT_PARAM, "HYB-E1002");

    // 3. Issue repro row 3: var init, result width int64 vs int8.
    const REPRO_VAR_RESULT: &str = r#"
fn g(x int) int64 {
    return 1
}
fn main() {
    var f fn(int) int8 = g
    _ = f
}
"#;
    rejects_with(REPRO_VAR_RESULT, "HYB-E1002");

    // 4. Result width, float family: float64 vs float32.
    const REPRO_RESULT_FLOAT: &str = r#"
fn g(x int) float64 {
    return 1.0
}
fn main() {
    var f fn(int) float32 = g
    _ = f
}
"#;
    rejects_with(REPRO_RESULT_FLOAT, "HYB-E1002");

    // 5. Width mismatch in the second of two results. (In type position
    // multiple results are space-separated: `fn(int) int int`.)
    const REPRO_MULTI_RESULT: &str = r#"
fn g(x int) (int, int64) {
    return 1, 2
}
fn main() {
    var f fn(int) int int = g
    _ = f
}
"#;
    rejects_with(REPRO_MULTI_RESULT, "HYB-E1002");

    // 6. Assignment statement (issue breadth: assignment position).
    const REPRO_ASSIGN: &str = r#"
fn g(x int64) int {
    return 1
}
fn h(x int) int {
    return 1
}
fn main() {
    f := h
    f = g
    _ = f
}
"#;
    rejects_with(REPRO_ASSIGN, "HYB-E1002");

    // 7. Call argument (issue breadth: call-argument position).
    const REPRO_CALL_ARG: &str = r#"
fn g(x int64) int {
    return 1
}
fn take(f fn(int) int) int {
    return f(1)
}
fn main() {
    _ = take(g)
}
"#;
    rejects_with(REPRO_CALL_ARG, "HYB-E1002");

    // 8. Return statement (issue breadth: return position).
    const REPRO_RETURN: &str = r#"
fn g(x int64) int {
    return 1
}
fn mk() fn(int) int {
    return g
}
"#;
    rejects_with(REPRO_RETURN, "HYB-E1002");

    // 9. Struct literal field (issue breadth: struct-literal position).
    const REPRO_STRUCT_FIELD: &str = r#"
struct runner {
    cb fn(int) int
}
fn g(x int64) int {
    return 1
}
fn main() {
    r := runner{cb: g}
    _ = r
}
"#;
    rejects_with(REPRO_STRUCT_FIELD, "HYB-E1002");

    // 10. Map value via index assignment (issue breadth: map-value position).
    const REPRO_MAP_VALUE: &str = r#"
fn g(x int64) int {
    return 1
}
fn main() {
    m := map[string]fn(int) int{}
    m["k"] = g
    _ = m
}
"#;
    rejects_with(REPRO_MAP_VALUE, "HYB-E1002");

    // 11. Fn literal (issue breadth: fn-literal position).
    const REPRO_FN_LIT: &str = r#"
fn main() {
    var f fn(int) int = fn(x int64) int { return 1 }
    _ = f
}
"#;
    rejects_with(REPRO_FN_LIT, "HYB-E1002");
}

#[test]
fn fn_scalar_width_matches_are_accepted() {
    // 1. Same width on both sides.
    const VALID_SAME_WIDTH: &str = r#"
fn g(x int64) int {
    return 1
}
fn main() {
    var f fn(int64) int = g
    _ = f(1)
}
"#;
    accepts(VALID_SAME_WIDTH);

    // 2. True aliases collapse: byte and uint8 are the same Go type.
    const VALID_BYTE_UINT8: &str = r#"
fn g(x uint8) int {
    return 1
}
fn main() {
    var f fn(byte) int = g
    _ = f(1)
}
"#;
    accepts(VALID_BYTE_UINT8);

    // 3. True aliases collapse in the result: rune and int32.
    const VALID_RUNE_INT32: &str = r#"
fn g(x int) int32 {
    return 1
}
fn main() {
    var f fn(int) rune = g
    _ = f(1)
}
"#;
    accepts(VALID_RUNE_INT32);

    // 4. Alias spelling: i64 is int64.
    const VALID_I64_SPELLING: &str = r#"
fn g(x int64) int {
    return 1
}
fn main() {
    var f fn(i64) int = g
    _ = f(1)
}
"#;
    accepts(VALID_I64_SPELLING);

    // 5. Declared type alias follows through to int64.
    const VALID_TYPE_ALIAS: &str = r#"
type my_int = int64
fn g(x int64) int {
    return 1
}
fn main() {
    var f fn(my_int) int = g
    _ = f(1)
}
"#;
    accepts(VALID_TYPE_ALIAS);

    // 6. Non-scalar elements still compare by kind only.
    const VALID_STRUCT_PARAM: &str = r#"
struct shape {
    n int
}
fn g(s shape) int {
    return 1
}
fn main() {
    var f fn(shape) int = g
    _ = f(shape{n: 1})
}
"#;
    accepts(VALID_STRUCT_PARAM);

    // 7. Multiple results with matching widths.
    const VALID_MULTI_RESULT: &str = r#"
fn g(x int) (int, int64) {
    return 1, 2
}
fn main() {
    var f fn(int) int int64 = g
    _ = f
}
"#;
    accepts(VALID_MULTI_RESULT);
}
