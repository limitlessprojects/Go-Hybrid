//! binary_operand_scalar_soundness.rs — regression tests for Issue #2877.
//!
//! Binary operations (arithmetic, bitwise, and comparisons) must verify that both
//! operands have matching concrete scalar types (e.g. rejecting `int32` vs `int64`,
//! `float32` vs `float64`), while permitting untyped constants and shift operations.

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
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn arithmetic_mismatched_scalars_rejected() {
    let ops = ["+", "-", "*", "/", "%", "&", "|", "^"];
    for op in ops {
        let src = format!(
            r#"
fn test_op() {{
    var a int32 = 1
    var b int64 = 2
    c := a {} b
    _ = c
}}
"#,
            op
        );
        let res = check(&src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "operator `{}` should reject mismatched int32 and int64",
            op
        );
        assert!(
            error_codes(&res).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{}`, got {:?}",
            op,
            res.diagnostics
        );
    }
}

#[test]
fn comparison_mismatched_scalars_rejected() {
    let comps = ["==", "!=", "<", "<=", ">", ">="];
    for cmp in comps {
        let src = format!(
            r#"
fn test_cmp() {{
    var a int32 = 1
    var b int64 = 2
    _ = a {} b
}}
"#,
            cmp
        );
        let res = check(&src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "comparison `{}` should reject mismatched int32 and int64",
            cmp
        );
        assert!(
            error_codes(&res).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{}`, got {:?}",
            cmp,
            res.diagnostics
        );
    }
}

#[test]
fn float_mismatched_scalars_rejected() {
    let src = r#"
fn test_float() {
    var f1 float32 = 1.0
    var f2 float64 = 2.0
    _ = f1 + f2
    _ = f1 == f2
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        res.diagnostics
    );
}

#[test]
fn untyped_constants_and_same_scalars_accepted() {
    let src = r#"
fn test_ok() {
    var a int32 = 10
    var b int32 = 20
    _ = a + b
    _ = a == b
    _ = a + 1
    _ = a == 10
    _ = a < 100
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid operations with matching scalars and untyped constants should pass: {:?}",
        res.diagnostics
    );
}

#[test]
fn shift_with_different_integer_type_accepted() {
    let src = r#"
fn test_shift() {
    var a int64 = 100
    var s uint32 = 2
    _ = a << s
    _ = a >> s
    _ = a << 1
}
"#;
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "shifts should allow differing integer scalar types: {:?}",
        res.diagnostics
    );
}

#[test]
fn nested_binary_scalar_mismatch_rejected() {
    let src = r#"
fn test_nested() {
    var a int32 = 1
    var b int64 = 2
    c := (a + 1) + b
    _ = c
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        error_codes(&res).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for nested binary scalar mismatch, got {:?}",
        res.diagnostics
    );
}
