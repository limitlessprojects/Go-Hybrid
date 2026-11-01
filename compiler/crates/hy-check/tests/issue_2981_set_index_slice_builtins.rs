//! Regression tests for Issue #2981:
//! Slicing, indexing, and calling cap, close, clear, copy on `set[T]`
//! must be rejected with HYB-E1002 or HYB-E2005 (Row 13 BUILDABLE).

use hy_check::check_file;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    hy_check::check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code)
        .collect()
}

fn assert_rejected(src: &str, expected_code: &str) {
    let codes = error_codes(src);
    assert!(
        codes.iter().any(|c| c == expected_code),
        "expected error code {expected_code}, got diagnostics {codes:?} for snippet:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let codes = error_codes(src);
    assert!(
        codes.is_empty(),
        "expected 0 errors, got {codes:?} for snippet:\n{src}"
    );
}

#[test]
fn test_set_index_rejected() {
    let src = r#"
fn main() {
    var s set[int]
    _ = s[0]
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_set_slice_rejected() {
    let src = r#"
fn main() {
    var s set[int]
    _ = s[0:1]
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn test_set_cap_rejected() {
    let src = r#"
fn main() {
    var s set[int]
    _ = cap(s)
}
"#;
    assert_rejected(src, "HYB-E2005");
}

#[test]
fn test_set_close_rejected() {
    let src = r#"
fn main() {
    var s set[int]
    close(s)
}
"#;
    assert_rejected(src, "HYB-E2005");
}

#[test]
fn test_set_clear_rejected() {
    let src = r#"
fn main() {
    var s set[int]
    clear(s)
}
"#;
    assert_rejected(src, "HYB-E2005");
}

#[test]
fn test_set_copy_rejected() {
    let src1 = r#"
fn main() {
    var s set[int]
    var xs []int
    copy(s, xs)
}
"#;
    assert_rejected(src1, "HYB-E2005");

    let src2 = r#"
fn main() {
    var s set[int]
    var xs []int
    copy(xs, s)
}
"#;
    assert_rejected(src2, "HYB-E2005");
}

#[test]
fn test_set_positive_controls_stay_accepted() {
    let src = r#"
fn main() {
    var s set[int]
    _ = len(s)
    s.insert(1)
    _ = s.contains(1)
    s.remove(1)

    var xs []int
    _ = cap(xs)
    clear(xs)

    var m map[string]int
    clear(m)

    ch := make(chan int, 1)
    close(ch)
}
"#;
    assert_ok(src);
}
