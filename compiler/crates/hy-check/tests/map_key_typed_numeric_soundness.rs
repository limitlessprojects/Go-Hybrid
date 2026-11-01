//! map_key_typed_numeric_soundness.rs — issue #2804 (Row 13 BUILDABLE).
//!
//! Go refuses map indexing (`m[k]`, `m[k] = v`) and `delete(m, k)` when the index/key
//! operand has a mismatched concrete numeric type:
//! - cannot use k (variable of type int) as int64 value in map index
//! - cannot use k (variable of type int) as int64 value in argument to delete
//! - cannot use f (variable of type float32) as float64 value in map index
//! - cannot use f (variable of type float32) as float64 value in argument to delete
//!
//! Untyped numeric constants and matching types/aliases (e.g. byte -> uint8, rune -> int32) stay accepted.

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
fn map_index_mismatched_concrete_numeric_rejects_e1002() {
    let cases = [
        (
            "int_var_to_int64_map_read",
            "\
fn main() {
    var m map[int64]string = map[int64]string{1: \"a\"}
    var k int = 1
    _ = m[k]
}
",
        ),
        (
            "int64_var_to_int_map_read",
            "\
fn main() {
    var m map[int]string = map[int]string{1: \"a\"}
    var k int64 = 1
    _ = m[k]
}
",
        ),
        (
            "float32_var_to_float64_map_read",
            "\
fn main() {
    var m map[float64]string = map[float64]string{1.0: \"a\"}
    var f float32 = 1.0
    _ = m[f]
}
",
        ),
        (
            "int_var_to_float64_map_read",
            "\
fn main() {
    var m map[float64]string = map[float64]string{1.0: \"a\"}
    var k int = 1
    _ = m[k]
}
",
        ),
        (
            "int_var_to_int64_map_write",
            "\
fn main() {
    var m map[int64]string = map[int64]string{}
    var k int = 1
    m[k] = \"b\"
}
",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case {name}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 for case {name}, got: {codes:?}"
        );
    }
}

#[test]
fn delete_mismatched_concrete_numeric_rejects_e2005() {
    let cases = [
        (
            "int_var_to_int64_map_delete",
            "\
fn main() {
    var m map[int64]string = map[int64]string{1: \"a\"}
    var k int = 1
    delete(m, k)
}
",
        ),
        (
            "int64_var_to_int_map_delete",
            "\
fn main() {
    var m map[int]string = map[int]string{1: \"a\"}
    var k int64 = 1
    delete(m, k)
}
",
        ),
        (
            "float32_var_to_float64_map_delete",
            "\
fn main() {
    var m map[float64]string = map[float64]string{1.0: \"a\"}
    var f float32 = 1.0
    delete(m, f)
}
",
        ),
        (
            "int_var_to_float64_map_delete",
            "\
fn main() {
    var m map[float64]string = map[float64]string{1.0: \"a\"}
    var k int = 1
    delete(m, k)
}
",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case {name}, got accept"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E2005".to_string()),
            "expected HYB-E2005 for case {name}, got: {codes:?}"
        );
    }
}

#[test]
fn map_key_accepts_untyped_constants_and_aliased_types() {
    let valid_cases = [
        (
            "untyped_int_to_int64_map",
            "\
fn main() {
    var m map[int64]string = map[int64]string{1: \"a\"}
    _ = m[42]
    delete(m, 42)
}
",
        ),
        (
            "untyped_int_to_float64_map",
            "\
fn main() {
    var m map[float64]string = map[float64]string{1.0: \"a\"}
    _ = m[42]
    delete(m, 42)
}
",
        ),
        (
            "byte_var_to_uint8_map",
            "\
fn main() {
    var m map[uint8]string = map[uint8]string{1: \"a\"}
    var b byte = 1
    _ = m[b]
    delete(m, b)
}
",
        ),
        (
            "rune_var_to_int32_map",
            "\
fn main() {
    var m map[int32]string = map[int32]string{1: \"a\"}
    var r rune = 1
    _ = m[r]
    delete(m, r)
}
",
        ),
        (
            "same_int_type",
            "\
fn main() {
    var m map[int]string = map[int]string{1: \"a\"}
    var k int = 1
    _ = m[k]
    delete(m, k)
}
",
        ),
    ];

    for (name, src) in valid_cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid case {name}, got reject: {:?}",
            r.diagnostics
        );
    }
}
