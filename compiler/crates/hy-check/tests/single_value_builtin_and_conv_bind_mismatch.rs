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
fn primitive_type_conversion_bound_to_multiple_vars_rejects_e1002() {
    let src = r#"
fn main() {
    x := 42
    a, b := int(x)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for a, b := int(x), got {:?}",
        r.diagnostics
    );
}

#[test]
fn single_value_builtin_len_bound_to_multiple_vars_rejects_e1002() {
    let src = r#"
fn main() {
    s := "hello"
    a, b := len(s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for a, b := len(s), got {:?}",
        r.diagnostics
    );
}

#[test]
fn single_value_builtin_make_bound_to_multiple_vars_rejects_e1002() {
    let src = r#"
fn main() {
    a, b := make([]int, 5)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for a, b := make(...), got {:?}",
        r.diagnostics
    );
}

#[test]
fn single_value_assign_target_mismatch_rejects_e1002() {
    let src = r#"
fn main() {
    var a int
    var b int
    a, b = int(42)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for a, b = int(42), got {:?}",
        r.diagnostics
    );
}

#[test]
fn normal_single_bindings_accepted() {
    let src = r#"
fn main() {
    a := int(42)
    b := len("hi")
    c := make([]int, 5)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(
        error_codes(&r).is_empty(),
        "expected 0 errors, got {:?}",
        r.diagnostics
    );
}
