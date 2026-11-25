//! issue_2688_set_methods_soundness.rs — regression tests for Issue #2688:
//! verifies that calling set[float64] methods (insert, contains, remove) or delete(s, v)
//! with typed int variables is rejected with HYB-E1002 / HYB-E2005, while untyped
//! integer constants and float expressions remain accepted (fixes #2688).

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
fn typed_int_in_set_float_insert_rejected_with_e1002() {
    let src = "\
fn main() {
    var s set[float64]
    var i int = 42
    s.insert(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.insert(i), got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_in_set_float_contains_rejected_with_e1002() {
    let src = "\
fn main() {
    var s set[float64]
    var i int = 42
    _ = s.contains(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.contains(i), got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_in_set_float_remove_rejected_with_e1002() {
    let src = "\
fn main() {
    var s set[float64]
    var i int = 42
    s.remove(i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for s.remove(i), got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_in_delete_set_float_rejected_with_e2005() {
    let src = "\
fn main() {
    var s set[float64]
    var i int = 42
    delete(s, i)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005 for delete(s, i), got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_constants_in_set_float_methods_and_delete_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20
    var s set[float64]
    s.insert(1)
    s.insert(file_k)
    s.insert(local_k)
    _ = s.contains(2)
    _ = s.contains(file_k)
    s.remove(3)
    s.remove(local_k)
    delete(s, 4)
    delete(s, file_k)
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped integer constants, got {:?}",
        r.diagnostics
    );
}

#[test]
fn matching_float_in_set_float_methods_and_delete_accepted() {
    let src = "\
fn main() {
    var s set[float64]
    var f float64 = 3.14
    s.insert(f)
    _ = s.contains(f)
    s.remove(f)
    delete(s, f)
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for matching float values, got {:?}",
        r.diagnostics
    );
}
