//! Field selectors through a pointer to a non-struct type must be rejected
//! (HYB-E1002, GUARANTEES.md Row 13 BUILDABLE).
//!
//! Go auto-dereferences `p.f` only when `p` points to a struct. For
//! `p := &n` with `n int` (or a string, slice, array, map, or channel),
//! `p.f` fails `go build` with `p.f undefined (type *int has no field or
//! method f)`. #3066 rejected selectors on the non-pointer forms; the pointer
//! forms took the struct path, found no struct name, and failed open for both
//! reads (`Expr::Field`) and stores (`PlaceKind::Field`). A multi-level
//! pointer (`pp **S`) has no fields or methods either: Go auto-dereferences
//! a selector through exactly one pointer.

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

#[test]
fn rejects_field_read_through_pointer_to_bool_and_chan() {
    assert_e1002(
        r#"
fn main() {
    ok := true
    p := &ok
    fmt.println(p.v)
}
"#,
    );
    assert_e1002(
        r#"
fn main() {
    c := make(chan int, 1)
    p := &c
    fmt.println(p.len)
}
"#,
    );
}

// Go auto-dereferences a selector through exactly one pointer: on `**S`,
// `pp.x` and `pp.m()` are both `pp.x undefined (type **S has no field or
// method x)`.

#[test]
fn rejects_field_read_through_double_pointer_to_struct() {
    assert_e1002(
        r#"
struct pt {
    x int
}
fn main() {
    v := pt{x: 1}
    p := &v
    pp := &p
    fmt.println(pp.x)
}
"#,
    );
}

#[test]
fn rejects_field_store_through_double_pointer_param() {
    assert_e1002(
        r#"
struct pt {
    x int
}
fn set(pp **pt) {
    pp.x = 2
}
fn main() {}
"#,
    );
}

#[test]
fn rejects_method_call_through_double_pointer_to_struct() {
    assert_e1002(
        r#"
struct pt {
    x int
}
fn (p *pt) get() int {
    return p.x
}
fn read(pp **pt) int {
    return pp.get()
}
fn main() {}
"#,
    );
}

#[test]
fn rejects_field_read_through_double_pointer_to_int() {
    assert_e1002(
        r#"
fn read(pp **int) {
    fmt.println(pp.foo)
}
fn main() {}
"#,
    );
}

#[test]
fn accepts_double_pointer_without_selector_and_single_pointer_method() {
    assert_ok(
        r#"
struct pt {
    x int
}
fn (p *pt) get() int {
    return p.x
}
fn pass(pp **pt) **pt {
    return pp
}
fn main() {
    v := pt{x: 1}
    p := &v
    pp := pass(&p)
    fmt.println(p.get(), p.x, pp == nil)
}
"#,
    );
}

// O8 slot loans: `p := &m[k]` / `&xs[i]` lower to a COPY of the element plus
// a store-back, so with a `*S` element `p` is a plain `*S` in Go and its
// fields stay legal (the checker sees `Pointer(Pointer(S))`).

#[test]
fn accepts_slot_loan_of_pointer_map_element_field_read_and_store() {
    assert_ok(
        r#"
struct item {
    f int
}
fn bump(m map[string]*item) {
    p := &m["a"]
    p.f = p.f + 1
}
fn main() {
    m := map[string]*item{"a": &item{f: 1}}
    bump(m)
    fmt.println(m["a"].f)
}
"#,
    );
}

#[test]
fn accepts_slot_loan_of_pointer_slice_element_field_read_and_store() {
    assert_ok(
        r#"
struct item {
    f int
}
fn bump(xs []*item) {
    q := &xs[0]
    q.f = q.f + 1
}
fn main() {
    xs := []*item{&item{f: 7}}
    bump(xs)
    fmt.println(xs[0].f)
}
"#,
    );
}

#[test]
fn rejects_slot_loan_of_double_pointer_element_field_read() {
    assert_e1002(
        r#"
struct item {
    f int
}
fn read(m map[string]**item) {
    p := &m["a"]
    fmt.println(p.f)
}
fn main() {}
"#,
    );
}

// Inside a closure the emitter has no active loans, so an alias of a captured
// loan (`q := p`) is emitted as written: a `*S` copy whose fields are legal.
#[test]
fn accepts_alias_of_captured_slot_loan_in_closure() {
    assert_ok(
        r#"
struct item {
    f int
}
fn bump(m map[string]*item) {
    p := &m["a"]
    g := fn() {
        q := p
        q.f = q.f + 1
    }
    g()
}
fn main() {
    m := map[string]*item{"a": &item{f: 1}}
    bump(m)
    fmt.println(m["a"].f)
}
"#,
    );
}

// Outside a closure an alias of a live loan is emitted as `q := &p` (a real
// `**S`), and `var` / if-init declarations are not slot-loan lowered at all
// (`var q = &xs[0]` stays a `**S`): Go rejects the selector in all three.
#[test]
fn rejects_selector_on_non_lowered_loans_of_pointer_elements() {
    assert_e1002(
        r#"
struct item {
    f int
}
fn read(m map[string]*item) {
    p := &m["a"]
    q := p
    fmt.println(q.f)
}
fn main() {}
"#,
    );
    assert_e1002(
        r#"
struct item {
    f int
}
fn read(xs []*item) {
    var q = &xs[0]
    fmt.println(q.f)
}
fn main() {}
"#,
    );
    assert_e1002(
        r#"
struct item {
    f int
}
fn read(xs []*item) {
    if q := &xs[0]; q.f > 0 {
        fmt.println(q.f)
    }
}
fn main() {}
"#,
    );
}
