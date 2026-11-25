//! Regression pin for #3292: map index and delete must compare the key
//! width, not just the collapsed `OpKind`.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("mapkey_3292.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// A map index whose key width differs from the declared key type.
#[test]
fn map_index_with_mismatched_key_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var m map[int32]string
    var k int64 = 1
    _ = m[k]
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// The same mismatch in a `delete` key.
#[test]
fn delete_with_mismatched_key_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var m map[int32]string
    var k int64 = 1
    delete(m, k)
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005, got {got:?}"
    );
}

/// Negative control: matching widths stay accepted, including an untyped
/// constant and a string key.
#[test]
fn matching_key_types_stay_accepted() {
    let got = error_codes(
        r#"fn main() {
    var m map[int32]string
    var k int32 = 1
    _ = m[k]
    delete(m, k)
    var m2 map[string]int
    m2["a"] = 1
    var m3 map[int]int
    m3[3] = 4
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}