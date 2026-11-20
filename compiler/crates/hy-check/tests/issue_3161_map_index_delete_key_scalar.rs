//! issue_3161_map_index_delete_key_scalar.rs — issue #3161 (Row 13 BUILDABLE).
//!
//! Map indexing (`m[k]`, `m[k] = v`) and `delete(m, k)` require the key
//! expression to match the map's declared key scalar width. Prior to this fix,
//! both `int32` and `int64` collapsed to `OpKind::Int`, allowing mismatched
//! scalar widths to pass `hyb check` and fail `go build`.

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
fn test_map_index_read_int_scalar_mismatch() {
    let src = r#"
fn main() {
    var m map[int32]string
    var k int64 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_write_int_scalar_mismatch() {
    let src = r#"
fn main() {
    var m map[int32]string
    var k int64 = 1
    m[k] = "val"
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_read_float_scalar_mismatch() {
    let src = r#"
fn main() {
    var m map[float32]string
    var k float64 = 1.0
    _ = m[k]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_delete_int_scalar_mismatch() {
    let src = r#"
fn main() {
    var m map[int32]string
    var k int64 = 1
    delete(m, k)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_delete_float_scalar_mismatch() {
    let src = r#"
fn main() {
    var m map[float32]string
    var k float64 = 1.0
    delete(m, k)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_short_decl_make_scalar_mismatch() {
    let src = r#"
fn main() {
    m := make(map[int32]string{})
    var k int64 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_short_decl_lit_scalar_mismatch() {
    let src = r#"
fn main() {
    m := map[int32]string{}
    var k int64 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_map_index_uint_mismatch() {
    let src = r#"
fn main() {
    var m map[uint32]string
    var k int32 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_set_delete_scalar_mismatch() {
    let src = r#"
fn main() {
    var s set[int32]
    var k int64 = 1
    delete(s, k)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_positive_matching_scalar_accepted() {
    let src = r#"
fn main() {
    var m map[int32]string
    var k int32 = 1
    _ = m[k]
    m[k] = "ok"
    delete(m, k)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

#[test]
fn test_positive_untyped_int_const_accepted() {
    let src = r#"
fn main() {
    var m map[int32]string
    _ = m[1]
    m[2] = "ok"
    delete(m, 3)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

#[test]
fn test_positive_untyped_float_const_accepted() {
    let src = r#"
fn main() {
    var m map[float32]string
    _ = m[1.5]
    m[2.5] = "ok"
    delete(m, 3.5)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

#[test]
fn test_positive_rune_alias_to_int32_accepted() {
    let src = r#"
fn main() {
    var m map[int32]string
    var r rune = 'a'
    _ = m[r]
    delete(m, r)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

#[test]
fn test_positive_byte_alias_to_uint8_accepted() {
    let src = r#"
fn main() {
    var m map[uint8]string
    var b byte = 1
    _ = m[b]
    delete(m, b)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}
