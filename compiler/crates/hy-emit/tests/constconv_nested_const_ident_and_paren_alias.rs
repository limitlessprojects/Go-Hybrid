//! Regression tests for SPEC X4 constant conversion folding & parenthesized/aliased
//! conversion callees in `hy-emit`:
//! 1. Nested fixed-width integer conversions (`u8(u16(300))`, `u8(i16(-1))`,
//!    `i8(u8(200))`, `u16(u8(200) + u8(100))`, `u8(int(300))`, `u8(uint(300))`)
//!    must fold when the inner constant or arithmetic expression overflows the
//!    target or inner Go constant type.
//! 2. File-level and block-scoped `const` identifiers referenced inside
//!    fixed-width integer conversions (`const big = 300; u8(big)`) must fold
//!    while respecting shadowing by local runtime variables.
//! 3. Type aliases to fixed-width integer types (`type my_byte = u8; my_byte(300)`)
//!    and 64-bit platform-word conversions (`uint(100 - 200)`) must fold.
//! 4. Parenthesized conversion and builtin callees (`(u8)(300)`, `(int8)(300)`,
//!    `(f64)(1)`, `([]byte)("abc")`, `(len)(s)`, `(delete)(s, 1)`) must peel
//!    parentheses so they lower identically to unparenthesized calls.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn nested_fixed_width_and_word_conversions_fold() {
    let go = emit_src(
        r#"
fn main() {
    a := u8(u16(300))
    b := u8(i16(-1))
    c := i8(u8(200))
    d := u16(u8(200) + u8(100))
    e := u8(int(300))
    f := u8(uint(300))
    g := uint(100 - 200)
    fmt.println("{a} {b} {c} {d} {e} {f} {g}")
}
"#,
    );
    assert!(
        go.contains("a := uint8(44)"),
        "expected u8(u16(300)) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("b := uint8(255)"),
        "expected u8(i16(-1)) -> uint8(255), got:\n{go}"
    );
    assert!(
        go.contains("c := int8(-56)"),
        "expected i8(u8(200)) -> int8(-56), got:\n{go}"
    );
    assert!(
        go.contains("d := uint16(300)"),
        "expected u16(u8(200) + u8(100)) -> uint16(300), got:\n{go}"
    );
    assert!(
        go.contains("e := uint8(44)"),
        "expected u8(int(300)) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("f := uint8(44)"),
        "expected u8(uint(300)) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("g := uint(18446744073709551516)"),
        "expected uint(100 - 200) -> uint(18446744073709551516), got:\n{go}"
    );
}

#[test]
fn file_and_local_const_identifiers_fold_with_shadowing() {
    let go = emit_src(
        r#"
const file_base = 200
const file_big = file_base + 100

fn main() {
    a := u8(file_big)
    const local_big = 300
    b := u8(local_big)
    {
        file_big := 7
        local_big := 9
        c := u8(file_big)
        d := u8(local_big)
        fmt.println("{c} {d}")
    }
    e := u8(local_big)
    fmt.println("{a} {b} {e}")
}
"#,
    );
    assert!(
        go.contains("a := uint8(44)"),
        "expected file-level const u8(file_big) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("b := uint8(44)"),
        "expected local const u8(local_big) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("c := uint8(file_big)"),
        "expected shadowed runtime variable file_big not to fold, got:\n{go}"
    );
    assert!(
        go.contains("d := uint8(local_big)"),
        "expected shadowed runtime variable local_big not to fold, got:\n{go}"
    );
    assert!(
        go.contains("e := uint8(44)"),
        "expected local const local_big to fold again after inner block restores scope, got:\n{go}"
    );
}

#[test]
fn type_alias_and_parenthesized_conversions_and_set_builtins_lower() {
    let go = emit_src(
        r#"
type my_byte = u8
type my_byte2 = my_byte

fn main() {
    a := my_byte(300)
    b := (my_byte2)(300)
    c := (u8)(300)
    d := (int8)(300)
    e := (f64)(1)
    f := ([]byte)("abc")
    g := ([]u8)("abc")
    s := set[int]{}
    s.insert(1)
    (delete)(s, 1)
    n := (len)(s)
    fmt.println("{a} {b} {c} {d} {e} {len(f)} {len(g)} {n}")
}
"#,
    );
    assert!(
        go.contains("a := my_byte(44)"),
        "expected my_byte(300) -> my_byte(44), got:\n{go}"
    );
    assert!(
        go.contains("b := my_byte2(44)"),
        "expected (my_byte2)(300) -> my_byte2(44), got:\n{go}"
    );
    assert!(
        go.contains("c := uint8(44)"),
        "expected (u8)(300) -> uint8(44), got:\n{go}"
    );
    assert!(
        go.contains("d := int8(44)"),
        "expected (int8)(300) -> int8(44), got:\n{go}"
    );
    assert!(
        go.contains("e := float64(1)"),
        "expected (f64)(1) -> float64(1), got:\n{go}"
    );
    assert!(
        go.contains("f := []byte(\"abc\")"),
        "expected ([]byte)(\"abc\") -> []byte(\"abc\"), got:\n{go}"
    );
    assert!(
        go.contains("g := []uint8(\"abc\")"),
        "expected ([]u8)(\"abc\") -> []uint8(\"abc\"), got:\n{go}"
    );
    assert!(
        go.contains("s.remove(1)"),
        "expected (delete)(s, 1) -> s.remove(1), got:\n{go}"
    );
    assert!(
        go.contains("n := s.len()"),
        "expected (len)(s) -> s.len(), got:\n{go}"
    );
}
