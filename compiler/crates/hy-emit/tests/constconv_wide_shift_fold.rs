//! Regression tests for `hy-emit` `constconv.rs`:
//! Chained or wide constant shifts inside fixed-width integer conversions
//! (`u8(...)`, `u16(...)`, `i8(...)`, `i16(...)`, etc.) must not silently
//! sign-corrupt at bit 127 or wrap away high bits via `i128::checked_shl`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn wide_chained_shifts_do_not_corrupt_sign_or_drop_bits_in_constconv() {
    let go = emit_src(
        r#"
fn main() {
    a := u16(((1 << 67) << 60) >> 120)
    b := u8(((1 << 70) << 60) >> 125)
    c := u16((3 << 126) >> 120)
    d := i8(((1 << 67) << 60) >> 120)
    e := u8((((1 << 70) << 60) >> 125) + 300)
    f := i16((-(1 << 70) << 60) >> 125)
    fmt.println("{a} {b} {c} {d} {e} {f}")
}
"#,
    );
    assert!(
        go.contains("a := uint16(128)"),
        "expected u16(((1 << 67) << 60) >> 120) -> uint16(128), got:\n{go}"
    );
    assert!(
        go.contains("b := uint8(32)"),
        "expected u8(((1 << 70) << 60) >> 125) -> uint8(32), got:\n{go}"
    );
    assert!(
        go.contains("c := uint16(192)"),
        "expected u16((3 << 126) >> 120) -> uint16(192), got:\n{go}"
    );
    assert!(
        go.contains("d := int8(-128)"),
        "expected i8(((1 << 67) << 60) >> 120) -> int8(-128), got:\n{go}"
    );
    assert!(
        go.contains("e := uint8(76)"),
        "expected u8((((1 << 70) << 60) >> 125) + 300) -> uint8(76), got:\n{go}"
    );
    assert!(
        go.contains("f := int16(-32)"),
        "expected i16((-(1 << 70) << 60) >> 125) -> int16(-32), got:\n{go}"
    );
}
