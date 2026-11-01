//! binary_operand_soundness.rs — regression tests for binary operations and compound assignments:
//! verifies that mixed operations between float64 and typed int variables are rejected with HYB-E1002,
//! while untyped integer constants and same-typed operations remain accepted (fixes #2650).

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
fn mixed_float_and_typed_int_variables_rejected_with_e1002() {
    let ops = ["+", "-", "*", "/", "<", "<=", ">", ">="];
    for op in ops {
        let src = format!(
            "fn main() {{\n    var f float64 = 1.0\n    var i int = 2\n    _ = f {} i\n}}\n",
            op
        );
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for `f {} i`, got accept",
            op
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for `f {} i`, got: {:?}",
            op,
            r.diagnostics
        );
    }
}

#[test]
fn mixed_float_and_typed_int_compound_assign_rejected_with_e1002() {
    let assign_ops = ["+=", "-=", "*=", "/="];
    for op in assign_ops {
        let src = format!(
            "fn main() {{\n    var f float64 = 1.0\n    var i int = 2\n    f {} i\n    _ = f\n}}\n",
            op
        );
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for `f {} i`, got accept",
            op
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for `f {} i`, got: {:?}",
            op,
            r.diagnostics
        );
    }
}

#[test]
fn untyped_constants_with_float_stay_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20
    var f float64 = 1.5
    _ = f + 1
    _ = f - 2
    _ = f * file_k
    _ = f / local_k
    _ = f < 100
    _ = f <= file_k
    _ = f > local_k
    _ = f >= 0
    f += 1
    f -= file_k
    f *= local_k
    f /= 2
    _ = f
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped integer constants with float, got diags: {:?}",
        r.diagnostics
    );
}

#[test]
fn same_type_operations_stay_accepted() {
    let src = "\
fn main() {
    var i1 int = 1
    var i2 int = 2
    var f1 float64 = 1.0
    var f2 float64 = 2.0
    var s1 string = \"a\"
    var s2 string = \"b\"
    _ = i1 + i2
    _ = i1 < i2
    _ = f1 + f2
    _ = f1 < f2
    _ = s1 + s2
    _ = s1 < s2
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for same-type operations, got diags: {:?}",
        r.diagnostics
    );
}
