//! Regression test for Issue #3112:
//! Field selectors and method calls on multiple pointer indirections (`**T.f`)
//! must be rejected (HYB-E1002).
//! Under GUARANTEES.md Row 13 (BUILDABLE), any program accepted by `hyb check`
//! must emit Go code that passes `go build`.

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
fn test_rejects_pointer_to_pointer_field_read() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    p := point{x: 1}
    ptr := &p
    pptr := &ptr
    _ = pptr.x
}
"#,
    );
}

#[test]
fn test_rejects_pointer_to_pointer_field_write() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    p := point{x: 1}
    ptr := &p
    pptr := &ptr
    pptr.x = 2
}
"#,
    );
}

#[test]
fn test_rejects_pointer_to_pointer_method_call() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn (p point) bump() {}
fn main() {
    p := point{x: 1}
    ptr := &p
    pptr := &ptr
    pptr.bump()
}
"#,
    );
}

#[test]
fn test_rejects_triple_pointer_field_selector() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    p := point{x: 1}
    ptr := &p
    pptr := &ptr
    ppptr := &pptr
    _ = ppptr.x
}
"#,
    );
}

#[test]
fn test_accepts_single_pointer_and_value_field_selectors() {
    assert_ok(
        r#"
struct point {
    x int
}
fn (p point) bump() {}
fn main() {
    p := point{x: 1}
    _ = p.x
    p.bump()
    ptr := &p
    _ = ptr.x
    ptr.bump()
}
"#,
    );
}
