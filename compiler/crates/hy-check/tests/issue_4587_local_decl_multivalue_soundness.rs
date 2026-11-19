//! Test suite for issue #4587:
//! Multi-value typed local declarations bypass type and scalar-width validation (HYB-E1002, Row 13).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn test_multivalue_kind_mismatch_rejected() {
    let src = r#"
fn pair() (string, string) {
    return "a", "b"
}

fn main() {
    var a, b int = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_scalar_width_mismatch_rejected() {
    let src = r#"
fn pair() (int32, int32) {
    return 1, 2
}

fn main() {
    var a, b int64 = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_comma_ok_map_mismatch_rejected() {
    let src = r#"
fn main() {
    m := make(map[string]int)
    var v, ok int = m["k"]
    _ = v
    _ = ok
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_comma_ok_chan_mismatch_rejected() {
    let src = r#"
fn main() {
    ch := make(chan string, 1)
    var v, ok string = <-ch
    _ = v
    _ = ok
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_let_kind_mismatch_rejected() {
    let src = r#"
fn pair() (string, string) {
    return "a", "b"
}

fn main() {
    let a, b int = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_fixed_array_len_mismatch_rejected() {
    let src = r#"
fn pair() ([3]int, [3]int) {
    return [3]int{1, 2, 3}, [3]int{4, 5, 6}
}

fn main() {
    var a, b [4]int = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_matching_types_accepted() {
    let src = r#"
fn pair() (int64, int64) {
    return 10, 20
}

fn main() {
    var a, b int64 = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_multivalue_matching_strings_accepted() {
    let src = r#"
fn pair() (string, string) {
    return "hello", "world"
}

fn main() {
    var a, b string = pair()
    _ = a
    _ = b
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.is_empty(),
        "expected 0 diagnostics, got {:?}",
        r.diagnostics
    );
}
