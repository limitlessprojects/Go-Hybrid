//! set_methods_soundness.rs — regression tests for set[T] method and delete soundness (fixes #2688).
//!
//! Validates:
//! 1. `s.insert(i)` on `set[float64]` with typed `int` variable rejected with HYB-E1002.
//! 2. `s.contains(i)` on `set[float64]` with typed `int` variable rejected with HYB-E1002.
//! 3. `s.remove(i)` on `set[float64]` with typed `int` variable rejected with HYB-E1002.
//! 4. `delete(s, i)` on `set[float64]` with typed `int` variable rejected with HYB-E2005.
//! 5. `s.unknown()` on `set[float64]` rejected with HYB-E1002 (no field or method).
//! 6. Untyped integer constants on `set[float64]` accepted cleanly.
//! 7. Matching typed variable on `set[int]` accepted cleanly.
//! 8. `delete(m, 1)` on `map[float64]int` with untyped int const accepted cleanly.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
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

#[test]
fn test_set_insert_typed_int_into_float_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    var i int = 42
    s.insert(i)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_set_contains_typed_int_into_float_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    var i int = 42
    _ = s.contains(i)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_set_remove_typed_int_into_float_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    var i int = 42
    s.remove(i)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_set_delete_typed_int_into_float_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    var i int = 42
    delete(s, i)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        codes
    );
}

#[test]
fn test_set_unknown_method_rejected() {
    let src = r#"
fn main() {
    var s set[float64]
    s.foobar()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection: {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes
    );
}

#[test]
fn test_set_untyped_int_const_accepted() {
    let src = r#"
fn main() {
    var s set[float64]
    s.insert(1)
    _ = s.contains(10)
    const c = 20
    s.remove(c)
    delete(s, 30)
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}

#[test]
fn test_set_matching_int_variable_accepted() {
    let src = r#"
fn main() {
    var s set[int]
    var i int = 42
    s.insert(i)
    _ = s.contains(i)
    s.remove(i)
    delete(s, i)
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}

#[test]
fn test_map_delete_untyped_int_const_accepted() {
    let src = r#"
fn main() {
    var m map[float64]int
    delete(m, 1)
    const k = 2
    delete(m, k)
}
"#;
    let r = check(src);
    let errors = error_codes(&r);
    assert!(errors.is_empty(), "expected 0 errors, got {:?}", errors);
}
