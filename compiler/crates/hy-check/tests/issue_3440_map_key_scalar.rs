use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("issue_3440.hy", src).expect("parse ok");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn issue_3440_inferred_map_binding_rejects_key_width_mismatch() {
    let src = r#"
fn main() {
    m := map[int32]string{1: "a"}
    var k int64 = 1
    _ = m[k]
    delete(m, k)
}
"#;
    let got = error_codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for m[k], got {:?}",
        got
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(m, k), got {:?}",
        got
    );
}

#[test]
fn issue_3440_inferred_make_map_binding_rejects_key_width_mismatch() {
    let src = r#"
fn main() {
    m := make(map[int32]string{})
    var k int64 = 1
    _ = m[k]
    delete(m, k)
}
"#;
    let got = error_codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for m[k], got {:?}",
        got
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(m, k), got {:?}",
        got
    );
}

#[test]
fn issue_3440_map_type_alias_rejects_key_width_mismatch() {
    let src = r#"
type int32_map = map[int32]string

fn main() {
    var m int32_map
    var k int64 = 1
    _ = m[k]
    delete(m, k)
}
"#;
    let got = error_codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for m[k], got {:?}",
        got
    );
    assert!(
        got.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(m, k), got {:?}",
        got
    );
}

#[test]
fn issue_3440_non_ident_map_base_rejects_key_width_mismatch() {
    let src = r#"
fn get_map() map[int32]string {
    return map[int32]string{1: "a"}
}

fn main() {
    var k int64 = 1
    _ = get_map()[k]
    _ = map[int32]string{1: "a"}[k]
}
"#;
    let got = error_codes(src);
    let count = got.iter().filter(|c| *c == "HYB-E1002").count();
    assert_eq!(
        count, 2,
        "expected 2 HYB-E1002 diagnostics for non-ident map bases, got {:?}",
        got
    );
}

#[test]
fn issue_3440_matching_key_width_passes() {
    let src = r#"
type int32_map = map[int32]string

fn main() {
    m := map[int32]string{1: "a"}
    var k int32 = 1
    _ = m[k]
    delete(m, k)

    var m2 int32_map
    _ = m2[k]
    delete(m2, k)
}
"#;
    let got = error_codes(src);
    assert!(
        got.is_empty(),
        "expected 0 errors for matching key widths, got {:?}",
        got
    );
}
