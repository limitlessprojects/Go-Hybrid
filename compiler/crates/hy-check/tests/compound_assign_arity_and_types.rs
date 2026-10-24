//! compound_assign_arity_and_types.rs — tests for HYB-E1002 rejection of
//! multi-target/multi-value compound assignments and operand type conflicts
//! (such as `int += float`, `int -= float`, etc.).

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
fn multi_target_compound_add_rejects_e1002() {
    let src = "fn main() {\n    a := 1\n    b := 2\n    a, b += 1, 2\n    _ = a\n    _ = b\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn multi_target_compound_sub_rejects_e1002() {
    let src = "fn main() {\n    a := 1\n    b := 2\n    a, b -= 1, 2\n    _ = a\n    _ = b\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn multi_value_compound_add_rejects_e1002() {
    let src = "fn main() {\n    a := 1\n    a += 1, 2\n    _ = a\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_add_float_lit_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    x += 2.5\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_sub_float_lit_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    x -= 2.5\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_mul_float_lit_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    x *= 2.5\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_div_float_lit_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    x /= 2.5\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_add_float_var_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    f := 2.5\n    x += f\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn int_compound_sub_float_var_rejects_e1002() {
    let src = "fn main() {\n    x := 10\n    f := 2.5\n    x -= f\n    _ = x\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_compound_assignments_accepted() {
    let src = "\
fn main() {
    x := 10
    x += 1
    x -= 1
    x *= 2
    x /= 2

    f := 1.5
    f += 1
    f -= 1
    f *= 2
    f /= 2
    f += 2.5

    s := \"hello \"
    s += \"world\"

    _ = x
    _ = f
    _ = s
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty(), "expected 0 errors, got {:?}", r.diagnostics);
}
