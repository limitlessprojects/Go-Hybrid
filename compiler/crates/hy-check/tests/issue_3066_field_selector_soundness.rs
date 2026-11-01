//! Regression test for Issue #3066:
//! Non-existent struct fields and field selectors on non-struct types must be rejected (HYB-E1002).
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
fn test_rejects_nonexistent_struct_field() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    pt := point{x: 1}
    println(pt.x)
    _ = pt.y
}
"#,
    );
}

#[test]
fn test_rejects_nonexistent_pointer_struct_field() {
    assert_e1002(
        r#"
struct point {
    x int
}
fn main() {
    p := &point{x: 1}
    println(p.x)
    _ = p.y
}
"#,
    );
}

#[test]
fn test_rejects_field_on_int() {
    assert_e1002(
        r#"
fn main() {
    x := 1
    println(x)
    _ = x.foo
}
"#,
    );
}

#[test]
fn test_rejects_field_on_bool() {
    assert_e1002(
        r#"
fn main() {
    b := true
    println(b)
    _ = b.foo
}
"#,
    );
}

#[test]
fn test_rejects_field_on_string() {
    assert_e1002(
        r#"
fn main() {
    s := "hello"
    println(s)
    _ = s.foo
}
"#,
    );
}

#[test]
fn test_rejects_field_on_chan() {
    assert_e1002(
        r#"
fn main() {
    ch := make(chan int)
    println(ch)
    _ = ch.foo
}
"#,
    );
}

#[test]
fn test_rejects_field_on_slice() {
    assert_e1002(
        r#"
fn main() {
    xs := []int{1}
    println(xs)
    _ = xs.foo
}
"#,
    );
}

#[test]
fn test_rejects_field_on_map() {
    assert_e1002(
        r#"
fn main() {
    m := map[string]int{}
    println(m)
    _ = m.foo
}
"#,
    );
}

#[test]
fn test_accepts_valid_field_selectors() {
    assert_ok(
        r#"
struct point {
    x int
}
fn main() {
    pt := point{x: 1}
    _ = pt.x
}
"#,
    );
    assert_ok(
        r#"
struct point {
    x int
}
fn main() {
    p := &point{x: 1}
    _ = p.x
}
"#,
    );
    assert_ok(
        r#"
struct point {
    x int
}
fn (p point) bump() {}
fn main() {
    pt := point{x: 1}
    _ = pt.bump
}
"#,
    );
    assert_ok(
        r#"
enum color {
    red,
    green
}
fn main() {
    _ = color.red
}
"#,
    );
}
