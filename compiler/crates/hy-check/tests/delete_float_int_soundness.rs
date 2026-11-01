//! delete_float_int_soundness.rs — tests for HYB-E2005 rejection
//! of typed integer variables used as float64 keys in `delete(m, k)`,
//! while preserving untyped integer constants adaptation.

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
fn delete_untyped_int_constants_on_float_map_accepted() {
    let src = r#"
const key_c = 42

fn test_delete_const() {
    var m map[float64]string
    delete(m, 1)
    delete(m, 0)
    delete(m, key_c)
    _ = m
}
"#;
    let r = check(src);
    let errs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == hy_diag::Severity::Error).collect();
    assert!(
        errs.is_empty(),
        "untyped int constants in delete(map[float64]T, ...) must be accepted, got errors: {:?}",
        errs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn delete_typed_int_variable_on_float_map_rejected() {
    let src = r#"
fn test_delete_var() {
    var m map[float64]string
    var i int = 1
    delete(m, i)
    _ = m
    _ = i
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(m, i), got {:?}",
        codes
    );
    assert!(r.diagnostics.iter().any(|d| d.message.contains("cannot use `i` (type `int`) as key type `float64` in `delete`")));
}

#[test]
fn delete_string_key_on_float_map_rejected() {
    let src = r#"
fn test_delete_str() {
    var m map[float64]string
    delete(m, "bad")
    _ = m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(m, \"bad\"), got {:?}",
        codes
    );
}
