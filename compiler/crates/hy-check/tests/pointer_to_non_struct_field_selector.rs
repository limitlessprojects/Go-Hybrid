//! Regression tests for field selectors through pointers to non-struct
//! and multi-level pointer types (HYB-E1002, GUARANTEES.md Row 13, Issue #4403).
//!
//! Go only auto-dereferences selectors (`p.f`) through a single pointer to a
//! struct (`*S`). Selectors through pointers to non-structs (`*int`, `*string`,
//! `*[]T`, `*map[K]V`, `*bool`, `*chan T`) and multi-level pointers (`**int`,
//! `**S`) must be rejected by `hyb check` for both reads and stores.

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
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn rejects_field_read_through_pointer_to_int() {
    assert_e1002(
        r#"
fn main() {
    x := 1
    p := &x
    fmt.println(p.foo)
}
"#,
    );
}

#[test]
fn rejects_field_store_through_pointer_to_int() {
    assert_e1002(
        r#"
fn main() {
    x := 1
    p := &x
    p.foo = 2
}
"#,
    );
}

#[test]
fn rejects_field_read_through_pointer_to_string() {
    assert_e1002(
        r#"
fn main() {
    s := "a"
    p := &s
    fmt.println(p.size)
}
"#,
    );
}

#[test]
fn rejects_field_read_and_store_through_pointer_to_slice() {
    assert_e1002(
        r#"
fn main() {
    s := []int{1}
    p := &s
    fmt.println(p.len)
}
"#,
    );
    assert_e1002(
        r#"
fn main() {
    s := []int{1}
    p := &s
    p.len = 3
}
"#,
    );
}

#[test]
fn rejects_field_read_and_store_through_pointer_to_map() {
    assert_e1002(
        r#"
fn main() {
    m := map[string]int{}
    p := &m
    fmt.println(p.k)
}
"#,
    );
    assert_e1002(
        r#"
fn main() {
    m := map[string]int{}
    p := &m
    p.k = 5
}
"#,
    );
}

#[test]
fn rejects_field_read_through_pointer_to_bool() {
    assert_e1002(
        r#"
fn f(p *bool) bool {
    return p.v
}
fn main() {
    b := true
    fmt.println(f(&b))
}
"#,
    );
}

#[test]
fn rejects_field_read_through_pointer_to_chan() {
    assert_e1002(
        r#"
fn main() {
    ch := make(chan int, 1)
    p := &ch
    fmt.println(p.buf)
}
"#,
    );
}

#[test]
fn rejects_field_read_through_double_pointer_to_int_and_struct() {
    assert_e1002(
        r#"
fn f(pp **int) {
    fmt.println(pp.foo)
}
fn main() {}
"#,
    );
    assert_e1002(
        r#"
struct pt {
    x int
}
fn f(pp **pt) {
    fmt.println(pp.x)
}
fn main() {}
"#,
    );
}

#[test]
fn accepts_pointer_to_struct_field_read_and_store() {
    assert_ok(
        r#"
struct pt {
    x int
}
fn main() {
    v := pt{x: 1}
    p := &v
    p.x = 3
    fmt.println(p.x)
}
"#,
    );
}

#[test]
fn accepts_pointer_to_generic_struct_field_read_and_store() {
    assert_ok(
        r#"
struct box[t any] {
    v t
}
fn main() {
    b := box[int]{v: 1}
    p := &b
    p.v = 2
    fmt.println(p.v)
}
"#,
    );
}
