//! Regression tests for #2474: set[T] method calls (.insert, .contains, .remove)
//! and delete(s, v) must check argument types against element type T.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
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

#[test]
fn valid_set_operations_pass() {
    let src = r#"
fn main() {
    s := set[int]{}
    _ = s.insert(1)
    _ = s.contains(1)
    s.remove(1)
    _ = s.len()
    delete(s, 1)
}
"#;
    let r = check(src);
    let errs = error_codes(&r);
    assert!(errs.is_empty(), "expected 0 errors for valid set operations, got: {:?}", errs);
}

#[test]
fn set_insert_incompatible_type_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "string_val"
    _ = s.insert(bad)
}
"#;
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.insert(bad) on set[int], got: {:?}",
        errs
    );
}

#[test]
fn set_contains_incompatible_type_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "string_val"
    _ = s.contains(bad)
}
"#;
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.contains(bad) on set[int], got: {:?}",
        errs
    );
}

#[test]
fn set_remove_incompatible_type_rejected_e1002() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "string_val"
    s.remove(bad)
}
"#;
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.remove(bad) on set[int], got: {:?}",
        errs
    );
}

#[test]
fn set_delete_incompatible_type_rejected_e2005() {
    let src = r#"
fn main() {
    s := set[int]{}
    bad := "string_val"
    delete(s, bad)
}
"#;
    let r = check(src);
    let errs = error_codes(&r);
    assert!(
        errs.contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005 for delete(s, bad) on set[int], got: {:?}",
        errs
    );
}