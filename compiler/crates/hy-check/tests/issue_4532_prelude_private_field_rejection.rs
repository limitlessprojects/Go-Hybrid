//! Regression test for Issue #4532:
//! Prelude-private fields of set[T] / option[T] (s.m, o.val, o.ok) must be rejected
//! with HYB-E1002 in Hybrid source (SPEC-LANGUAGE T9 / T20).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn test_rejects_set_private_field_read() {
    assert_e1002(
        r#"
fn main() {
    s := set[int]{}
    _ = s.insert(7)
    _ = s.m
}
"#,
    );
}

#[test]
fn test_rejects_option_private_field_read() {
    assert_e1002(
        r#"
fn main() {
    o := some(3)
    _ = o.val
}
"#,
    );
    assert_e1002(
        r#"
fn main() {
    o := some(3)
    _ = o.ok
}
"#,
    );
}

#[test]
fn test_rejects_option_private_field_write() {
    assert_e1002(
        r#"
fn get() option[int] {
    return none
}
fn main() {
    o := get()
    o.val = 42
}
"#,
    );
    assert_e1002(
        r#"
fn get() option[int] {
    return none
}
fn main() {
    o := get()
    o.ok = true
}
"#,
    );
}

#[test]
fn test_rejects_none_private_field_read() {
    assert_e1002(
        r#"
fn main() {
    _ = none.val
}
"#,
    );
}

#[test]
fn test_accepts_valid_set_and_option_methods() {
    assert_ok(
        r#"
fn main() {
    s := set[int]{}
    _ = s.insert(7)
    _ = s.contains(7)
    _ = s.remove(7)
    o := some(3)
    if o.is_some() {
        _ = o.unwrap()
    }
}
"#,
    );
}
