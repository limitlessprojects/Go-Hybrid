//! issue_3292_map_key_scalar_soundness.rs — tests for HYB-E1002 / HYB-E2005 rejection when
//! map index or delete key has mismatched numerical scalar width.

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
fn map_int32_read_index_with_int64_rejects_e1002() {
    let src = "\
fn main() {
    var m map[int32]string
    var k int64 = 1
    _ = m[k]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_int32_write_index_with_int64_rejects_e1002() {
    let src = "\
fn main() {
    var m map[int32]string
    var k int64 = 1
    m[k] = \"hello\"
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_int32_delete_with_int64_rejects_e2005() {
    let src = "\
fn main() {
    var m map[int32]string
    var k int64 = 1
    delete(m, k)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_signed_int_with_uint_key_rejects_e1002() {
    let src = "\
fn main() {
    var m map[int]string
    var k uint = 1
    _ = m[k]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_float32_with_float64_index_rejects_e1002() {
    let src = "\
fn main() {
    var m map[float32]string
    var f float64 = 1.5
    _ = m[f]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_float32_with_float64_delete_rejects_e2005() {
    let src = "\
fn main() {
    var m map[float32]string
    var f float64 = 1.5
    delete(m, f)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E2005".to_string()),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn map_matching_int32_key_accepted() {
    let src = "\
fn main() {
    var m map[int32]string
    var k int32 = 1
    _ = m[k]
    m[k] = \"ok\"
    delete(m, k)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn map_untyped_int_constant_accepted() {
    let src = "\
fn main() {
    var m map[int32]string
    _ = m[1]
    m[1] = \"ok\"
    delete(m, 1)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn map_literal_with_mismatched_key_rejects_e1002() {
    let src = "\
fn main() {
    m := map[int32]string{}
    var k int64 = 2
    _ = m[k]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}
