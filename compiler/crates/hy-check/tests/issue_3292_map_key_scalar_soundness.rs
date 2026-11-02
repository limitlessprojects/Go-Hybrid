//! Regression tests for issue #3292:
//! hyb-check: enforce map key scalar width in map index and delete (HYB-E1002 / HYB-E2005)

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn test_map_read_index_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var m map[int32]string
    var k int64 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject mismatched map key scalar in read index");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("cannot use `k`")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_write_index_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var m map[int32]string
    var k int64 = 1
    m[k] = "hello"
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject mismatched map key scalar in assignment index");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("cannot use `k`")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_delete_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var m map[int32]string
    var k int64 = 1
    delete(m, k)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject mismatched key scalar in delete");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E2005") && e.contains("delete")),
        "expected HYB-E2005, got: {:?}",
        errs
    );
}

#[test]
fn test_map_struct_field_index_scalar_mismatch_rejected() {
    let src = r#"package main

struct container {
    lookup map[int32]string
}

fn main() {
    var c container
    var k int64 = 1
    _ = c.lookup[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject struct field map index scalar mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("cannot use `k`")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_struct_field_delete_scalar_mismatch_rejected() {
    let src = r#"package main

struct container {
    lookup map[int32]string
}

fn main() {
    var c container
    var k int64 = 1
    delete(c.lookup, k)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject struct field map delete scalar mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E2005") && e.contains("delete")),
        "expected HYB-E2005, got: {:?}",
        errs
    );
}

#[test]
fn test_map_type_alias_scalar_mismatch_rejected() {
    let src = r#"package main

type table = map[int16]bool

fn main() {
    var t table
    var k int32 = 5
    _ = t[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject type-aliased map key scalar mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains("cannot use `k`")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_uint_vs_int_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var m map[uint]string
    var k int = 1
    _ = m[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject uint vs int map key mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_float_scalar_mismatch_rejected() {
    let src = r#"package main

fn main() {
    var m map[float32]string
    var k float64 = 1.0
    _ = m[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject float32 vs float64 map key mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_map_short_decl_literal_index_mismatch_rejected() {
    let src = r#"package main

fn main() {
    m := map[int32]string{1: "val"}
    var k int64 = 1
    _ = m[k]
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject short decl literal map scalar mismatch");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002")),
        "expected HYB-E1002, got: {:?}",
        errs
    );
}

#[test]
fn test_bracketed_map_key_type_parses_safely() {
    let src = r#"package main

fn main() {
    var m map[[2]int32]string
    var k [2]int32
    _ = m[k]
    delete(m, k)
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Accept, "bracketed map key type must parse safely, got: {:?}", errs);
}

#[test]
fn test_matching_key_types_accepted() {
    let src = r#"package main

struct container {
    lookup map[int32]string
}

fn main() {
    var m map[int32]string
    var k int32 = 1
    _ = m[k]
    m[k] = "val"
    delete(m, k)

    // Untyped numeric constants are assignable to integer key
    _ = m[42]
    m[42] = "answer"
    delete(m, 42)

    var c container
    _ = c.lookup[k]
    delete(c.lookup, k)

    var m_str map[string]int
    _ = m_str["key"]
    delete(m_str, "key")
}
"#;
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Accept, "valid map indexing and delete must pass, got: {:?}", errs);
}
