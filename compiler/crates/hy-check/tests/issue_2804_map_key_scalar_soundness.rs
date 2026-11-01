//! issue_2804_map_key_scalar_soundness.rs — tests for Issue #2804 (Row 13 BUILDABLE).
//!
//! Go unconditionally rejects map indexing and delete with mismatched concrete
//! numeric scalar key types:
//! - cannot use k (variable of type int) as int64 value in map index
//! - cannot use k (variable of type int) as int64 value in argument to delete
//! - cannot use f (variable of type float32) as float64 value in map index
//! - cannot use f (variable of type float32) as float64 value in argument to delete
//!
//! Untyped numeric constants and matching types/aliases (e.g. byte -> uint8, rune -> int32)
//! stay accepted.

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
fn map_index_typed_numeric_width_mismatch_rejected_e1002() {
    let cases = [
        (
            "int_var_to_int64_map_index",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int = 1
    _ = m[k]
}
"#,
        ),
        (
            "int32_var_to_int64_map_index",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int32 = 1
    _ = m[k]
}
"#,
        ),
        (
            "int64_var_to_int32_map_index",
            r#"
fn main() {
    var m map[int32]string = map[int32]string{1: "a"}
    var k int64 = 1
    _ = m[k]
}
"#,
        ),
        (
            "float32_var_to_float64_map_index",
            r#"
fn main() {
    var m map[float64]string = map[float64]string{1.0: "a"}
    var f float32 = 1.0
    _ = m[f]
}
"#,
        ),
        (
            "float64_var_to_float32_map_index",
            r#"
fn main() {
    var m map[float32]string = map[float32]string{1.0: "a"}
    var f float64 = 1.0
    _ = m[f]
}
"#,
        ),
        (
            "assign_target_map_index_mismatch",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int = 1
    m[k] = "bad"
}
"#,
        ),
        (
            "inferred_map_lit_index_mismatch",
            r#"
fn main() {
    m := map[int64]string{1: "a"}
    var k int = 1
    _ = m[k]
}
"#,
        ),
        (
            "empty_map_lit_index_mismatch",
            r#"
fn main() {
    m := map[int64]string{}
    var k int = 1
    _ = m[k]
}
"#,
        ),
        (
            "struct_field_map_index_mismatch",
            r#"
struct container {
    m map[int64]string
}

fn main() {
    var c container
    var k int = 1
    _ = c.m[k]
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should be rejected, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E1002".to_string()),
            "case {name} expected HYB-E1002, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn delete_typed_numeric_width_mismatch_rejected_e2005() {
    let cases = [
        (
            "int_var_to_int64_map_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int = 1
    delete(m, k)
}
"#,
        ),
        (
            "int32_var_to_int64_map_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int32 = 1
    delete(m, k)
}
"#,
        ),
        (
            "int64_var_to_int32_map_delete",
            r#"
fn main() {
    var m map[int32]string = map[int32]string{1: "a"}
    var k int64 = 1
    delete(m, k)
}
"#,
        ),
        (
            "float32_var_to_float64_map_delete",
            r#"
fn main() {
    var m map[float64]string = map[float64]string{1.0: "a"}
    var f float32 = 1.0
    delete(m, f)
}
"#,
        ),
        (
            "float64_var_to_float32_map_delete",
            r#"
fn main() {
    var m map[float32]string = map[float32]string{1.0: "a"}
    var f float64 = 1.0
    delete(m, f)
}
"#,
        ),
        (
            "inferred_map_lit_delete_mismatch",
            r#"
fn main() {
    m := map[int64]string{1: "a"}
    var k int = 1
    delete(m, k)
}
"#,
        ),
        (
            "empty_map_lit_delete_mismatch",
            r#"
fn main() {
    m := map[int64]string{}
    var k int = 1
    delete(m, k)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {name} should be rejected, got Accept"
        );
        assert!(
            error_codes(&r).contains(&"HYB-E2005".to_string()),
            "case {name} expected HYB-E2005, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn compatible_map_keys_and_untyped_constants_accepted() {
    let cases = [
        (
            "matching_int64_var_index_and_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    var k int64 = 1
    _ = m[k]
    delete(m, k)
}
"#,
        ),
        (
            "matching_float64_var_index_and_delete",
            r#"
fn main() {
    var m map[float64]string = map[float64]string{1.0: "a"}
    var f float64 = 1.0
    _ = m[f]
    delete(m, f)
}
"#,
        ),
        (
            "untyped_int_literal_index_and_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{1: "a"}
    _ = m[1]
    delete(m, 1)
}
"#,
        ),
        (
            "untyped_int_zero_index_and_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{0: "a"}
    _ = m[0]
    delete(m, 0)
}
"#,
        ),
        (
            "untyped_int_const_on_float_map",
            r#"
fn main() {
    var m map[float64]string = map[float64]string{1.0: "a"}
    _ = m[1]
    delete(m, 1)
}
"#,
        ),
        (
            "untyped_float_literal_on_float_map",
            r#"
fn main() {
    var m map[float64]string = map[float64]string{1.5: "a"}
    _ = m[1.5]
    delete(m, 1.5)
}
"#,
        ),
        (
            "untyped_binary_arith_expr_index_and_delete",
            r#"
fn main() {
    var m map[int64]string = map[int64]string{3: "a"}
    _ = m[1 + 2]
    delete(m, 1 + 2)
}
"#,
        ),
        (
            "byte_and_uint8_matching_alias",
            r#"
fn main() {
    var m map[uint8]string = map[uint8]string{1: "a"}
    var b byte = 1
    _ = m[b]
    delete(m, b)
}
"#,
        ),
        (
            "rune_and_int32_matching_alias",
            r#"
fn main() {
    var m map[int32]string = map[int32]string{1: "a"}
    var r rune = 1
    _ = m[r]
    delete(m, r)
}
"#,
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "case {name} expected Accept, got Reject with diagnostics: {:?}",
            r.diagnostics
        );
        assert!(
            error_codes(&r).is_empty(),
            "case {name} expected 0 errors, got {:?}",
            r.diagnostics
        );
    }
}
