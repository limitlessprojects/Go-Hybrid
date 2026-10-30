//! field_and_void_operand_soundness.rs — tests for undefined field access/assignment
//! and void-call operands in composite literals and channel sends (HYB-E1002 / Row 13 BUILDABLE).
//!
//! Validates:
//! 1. Undefined field selection on structs: `p.missing` -> HYB-E1002
//! 2. Field selection on non-struct types: `x.nope` -> HYB-E1002
//! 3. Undefined field assignment on structs: `p.missing = 10` -> HYB-E1002
//! 4. Field assignment on non-struct types: `x.nope = 20` -> HYB-E1002
//! 5. Assignment to method on struct: `p.add = 10` -> HYB-E1002
//! 6. Field selection on void calls: `void_fn().x` -> HYB-E1002
//! 7. Void call in slice literal: `[]int{void_fn()}` -> HYB-E1002
//! 8. Void call in map literal key/value: `map[string]int{"k": void_fn()}` -> HYB-E1002
//! 9. Void call in struct literal: `point{x: void_fn()}` -> HYB-E1002
//! 10. Void call in channel send: `ch <- void_fn()` -> HYB-E1002
//! 11. Controls: valid field access, valid field assignment, method values, pointer receivers, package imports.

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

fn assert_has_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn undefined_field_on_struct_rejected() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    p := point{x: 1, y: 2}
    v := p.missing
    fmt.println("{v}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn field_on_primitive_rejected() {
    let src1 = r#"
fn main() {
    x := 5
    v := x.nope
    fmt.println("{v}")
}
"#;
    assert_has_code(src1, "HYB-E1002");

    let src2 = r#"
fn main() {
    b := true
    v := b.nope
    fmt.println("{v}")
}
"#;
    assert_has_code(src2, "HYB-E1002");

    let src3 = r#"
fn main() {
    s := "hello"
    v := s.nope
    fmt.println("{v}")
}
"#;
    assert_has_code(src3, "HYB-E1002");
}

#[test]
fn undefined_field_assignment_on_struct_rejected() {
    let src = r#"
struct point {
    x int
    y int
}

fn main() {
    p := point{x: 1, y: 2}
    p.missing = 10
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn field_assignment_on_primitive_rejected() {
    let src = r#"
fn main() {
    x := 5
    x.nope = 20
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn assign_to_method_rejected() {
    let src = r#"
struct point {
    x int
}

fn (p *point) add(k int) int {
    return p.x + k
}

fn main() {
    p := point{x: 1}
    p.add = 10
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn field_access_on_void_call_rejected() {
    let src = r#"
fn void_fn() {}

fn main() {
    _ = void_fn().x
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn void_call_in_composite_literals_rejected() {
    let src1 = r#"
fn void_fn() {}

fn main() {
    xs := []int{void_fn()}
    fmt.println("{xs}")
}
"#;
    assert_has_code(src1, "HYB-E1002");

    let src2 = r#"
fn void_fn() {}

fn main() {
    m := map[string]int{"k": void_fn()}
    fmt.println("{m}")
}
"#;
    assert_has_code(src2, "HYB-E1002");

    let src3 = r#"
fn void_fn() {}

fn main() {
    m := map[int]string{void_fn(): "v"}
    fmt.println("{m}")
}
"#;
    assert_has_code(src3, "HYB-E1002");

    let src4 = r#"
struct point {
    x int
    y int
}

fn void_fn() {}

fn main() {
    p := point{x: void_fn(), y: 10}
    fmt.println("{p.y}")
}
"#;
    assert_has_code(src4, "HYB-E1002");
}

#[test]
fn void_call_in_channel_send_rejected() {
    let src = r#"
fn void_fn() {}

fn main() {
    ch := make(chan int)
    managed("test", "other") {
        ch <- void_fn()
    }
    fmt.println("{ch}")
}
"#;
    assert_has_code(src, "HYB-E1002");
}

#[test]
fn valid_field_and_composite_controls_accepted() {
    let src = r#"
struct point {
    x int
    y int
}

fn (p *point) get_x() int {
    return p.x
}

fn main() {
    p := point{x: 1, y: 2}
    v := p.x
    p.x = 10
    method_val := p.get_x
    ptr := &p
    ptr_v := ptr.y
    ptr.y = 20
    xs := []int{1, 2, 3}
    m := map[string]int{"a": 1, "b": 2}
    fmt.println("{v}", "{p.x}", "{ptr_v}", "{ptr.y}", "{xs}", "{m}")
    _ = method_val
}
"#;
    assert_ok(src);
}
