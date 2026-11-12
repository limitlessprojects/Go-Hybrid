//! Field selectors through a pointer to a non-struct type must be rejected
//! (HYB-E1002, GUARANTEES.md Row 13 BUILDABLE).
//!
//! Go auto-dereferences `p.f` only when `p` points to a struct. For
//! `p := &n` with `n int` (or a string, slice, array, map, or channel),
//! `p.f` fails `go build` with `p.f undefined (type *int has no field or
//! method f)`. #3066 rejected selectors on the non-pointer forms; the pointer
//! forms took the struct path, found no struct name, and failed open for both
//! reads (`Expr::Field`) and stores (`PlaceKind::Field`).

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
    y := &x
    fmt.println(y.foo)
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
fn rejects_field_read_through_pointer_to_map() {
    assert_e1002(
        r#"
fn main() {
    m := map[string]int{}
    p := &m
    fmt.println(p.k)
}
"#,
    );
}

#[test]
fn rejects_field_store_through_pointer_to_slice() {
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
fn rejects_field_read_through_pointer_param() {
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
fn accepts_pointer_to_scalar_without_selector() {
    assert_ok(
        r#"
fn main() {
    x := 1
    y := &x
    fmt.println(y)
}
"#,
    );
}

#[test]
fn accepts_pointer_to_generic_struct_field() {
    assert_ok(
        r#"
struct box[t any] {
    v t
}
fn main() {
    b := box[int]{v: 1}
    p := &b
    fmt.println(p.v)
}
"#,
    );
}
