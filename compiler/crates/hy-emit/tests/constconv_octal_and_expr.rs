//! Regression tests for `hy-emit` `constconv.rs`:
//! 1. Legacy leading-zero octal literals (`0377` = 255, `0200` = 128, `0400` = 256)
//!    must be parsed in base 8, not base 10:
//!    - `u8(0377)` fits in `u8` (255) and must NOT be corrupted to `uint8(121)` (`377 & 0xFF`).
//!    - `i8(0200)` (128) must truncate to `int8(-128)`, not `int8(-56)` (`200 as i8`).
//!    - `u8(0400)` (256) must truncate to `uint8(0)`, not `uint8(144)` (`400 & 0xFF`).
//! 2. Constant unary (`~`), binary (`+`, `-`, `*`, `/`, `%`, `<<`, `>>`, `&`, `|`, `^`),
//!    and rune (`'…'`) expressions in fixed-width conversions must fold per X4
//!    instead of emitting out-of-range Go constant expressions that fail `go build`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn legacy_octal_literals_do_not_miscompile_as_decimal_in_constconv() {
    let go = emit_src(
        r#"
fn main() {
    a := u8(0377)
    b := i8(0200)
    c := u8(0400)
    d := u8(0777)
    fmt.println("{a} {b} {c} {d}")
}
"#,
    );
    assert!(
        go.contains("a := uint8(0377)"),
        "0377 (octal 255) fits in u8 and must not be corrupted to uint8(121), got:\n{go}"
    );
    assert!(
        go.contains("b := int8(-128)"),
        "0200 (octal 128) must fold to int8(-128), got:\n{go}"
    );
    assert!(
        go.contains("c := uint8(0)"),
        "0400 (octal 256) must fold to uint8(0), got:\n{go}"
    );
    assert!(
        go.contains("d := uint8(255)"),
        "0777 (octal 511) must fold to uint8(255), got:\n{go}"
    );
}

#[test]
fn constant_binary_and_rune_expressions_fold_in_constconv() {
    let go = emit_src(
        r#"
fn main() {
    a := u8(200 + 100)
    b := int8(1 << 10)
    c := uint8(10 - 20)
    d := u8(0x1FF & 0x12C)
    e := u8(300 - 250)
    f := i8('\u{00FF}')
    g := u8(10 + 20)
    fmt.println("{a} {b} {c} {d} {e} {f} {g}")
}
"#,
    );
    assert!(
        go.contains("a := uint8(44)"),
        "expected u8(200 + 100) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("b := int8(0)"),
        "expected int8(1 << 10) -> int8(0), got:\n{go}"
    );
    assert!(
        go.contains("c := uint8(246)"),
        "expected uint8(10 - 20) -> uint8(246), got:\n{go}"
    );
    assert!(
        go.contains("d := uint8(44)"),
        "expected u8(0x1FF & 0x12C) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("e := uint8(50)"),
        "expected u8(300 - 250) -> uint8(50), got:\n{go}"
    );
    assert!(
        go.contains("f := int8(-1)"),
        "expected i8('\\u{{00FF}}') -> int8(-1), got:\n{go}"
    );
    assert!(
        go.contains("g := uint8((10 + 20))") || go.contains("g := uint8(10 + 20)"),
        "expected in-range u8(10 + 20) to stay unfolded, got:\n{go}"
    );
}
