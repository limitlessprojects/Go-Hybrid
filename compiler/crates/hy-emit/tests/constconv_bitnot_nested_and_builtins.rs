//! Regression tests for `hy-emit` `constconv.rs`:
//! 1. Bitwise NOT (`^x`) and `move` inside fixed-width integer conversions
//!    (`u8(^0)` -> `uint8(255)`, `u16(^1)` -> `uint16(65534)`,
//!    `i8(^255)` -> `int8(0)`, `u8(move 300)` -> `uint8(44)`).
//! 2. Nested integer type conversions inside fixed-width conversions
//!    (`i8(u8(200))` -> `int8(-56)`, `u8(i8(-1))` -> `uint8(255)`,
//!    `u16(u8(200) + 100)` -> `uint16(300)`).
//! 3. Compile-time constant builtin calls `len("...")`, `min(...)`, and
//!    `max(...)` inside fixed-width conversions (`u8(len("abc") - 10)` ->
//!    `uint8(249)`, `u8(max(300, 10))` -> `uint8(44)`,
//!    `u8(min(-1, 5))` -> `uint8(255)`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn bitnot_and_move_constants_fold_in_fixed_width_conversions() {
    let go = emit_src(
        r#"
fn main() {
    a := u8(^0)
    b := u16(^1)
    c := i8(^255)
    d := u8(^0 + 300)
    e := u8(move 300)
    fmt.println("{a} {b} {c} {d} {e}")
}
"#,
    );
    assert!(go.contains("a := uint8(255)"), "expected uint8(255), got:\n{go}");
    assert!(go.contains("b := uint16(65534)"), "expected uint16(65534), got:\n{go}");
    assert!(go.contains("c := int8(0)"), "expected int8(0), got:\n{go}");
    assert!(go.contains("d := uint8(43)"), "expected uint8(43), got:\n{go}");
    assert!(go.contains("e := uint8(44)"), "expected uint8(44), got:\n{go}");
}

#[test]
fn nested_integer_conversions_fold_in_fixed_width_conversions() {
    let go = emit_src(
        r#"
fn main() {
    a := i8(u8(200))
    b := u8(i8(-1))
    c := u8(i16(300))
    d := u16(u8(200) + 100)
    e := u8(i8(-10) + 5)
    fmt.println("{a} {b} {c} {d} {e}")
}
"#,
    );
    assert!(go.contains("a := int8(-56)"), "expected int8(-56), got:\n{go}");
    assert!(go.contains("b := uint8(255)"), "expected uint8(255), got:\n{go}");
    assert!(go.contains("c := uint8(44)"), "expected uint8(44), got:\n{go}");
    assert!(go.contains("d := uint16(300)"), "expected uint16(300), got:\n{go}");
    assert!(go.contains("e := uint8(251)"), "expected uint8(251), got:\n{go}");
}

#[test]
fn constant_len_min_and_max_builtins_fold_in_fixed_width_conversions() {
    let go = emit_src(
        r#"
fn main() {
    a := u8(len("abc") - 10)
    b := u8(len("ab" + "cd") + 296)
    c := u8(max(300, 10))
    d := u8(min(-1, 5))
    e := i8(max(200, 50))
    fmt.println("{a} {b} {c} {d} {e}")
}
"#,
    );
    assert!(go.contains("a := uint8(249)"), "expected uint8(249), got:\n{go}");
    assert!(go.contains("b := uint8(44)"), "expected uint8(44), got:\n{go}");
    assert!(go.contains("c := uint8(44)"), "expected uint8(44), got:\n{go}");
    assert!(go.contains("d := uint8(255)"), "expected uint8(255), got:\n{go}");
    assert!(go.contains("e := int8(-56)"), "expected int8(-56), got:\n{go}");
}
