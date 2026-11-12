//! Regression tests for `constconv.rs` and `expr.rs` ensuring that
//! `fold_constant_with` does not fold inner `len`, `min`, `max`, or fixed-width
//! conversion calls (`u8`, `i8`, ...) when those names are shadowed by a
//! user-declared function or an in-scope local/parameter binding.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parses");
    emit_go(&file).expect("emits")
}

#[test]
fn does_not_fold_shadowed_user_functions_inside_fixed_width_conversions() {
    let go = emit(
        r#"package main

import "fmt"

fn len(s string) int {
    return 50
}

fn min(a int, b int) int {
    return a + b
}

fn u8(x int) int {
    return x + 10
}

fn main() {
    a := u16(len("abc") + 1)
    b := u16(min(1, 2))
    c := u16(u8(5))
    fmt.println(a, b, c)
}
"#,
    );
    assert!(
        go.contains("a := uint16(len(\"abc\") + 1)")
            && go.contains("b := uint16(min(1, 2))")
            && go.contains("c := uint16(u8(5))"),
        "expected shadowed user functions `len`, `min`, `u8` to be emitted as runtime calls instead of constant-folded:\n{go}"
    );
}

#[test]
fn does_not_erase_local_closure_shadowing_builtin_inside_conversion() {
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    min := fn(a int, b int) int {
        return 99
    }
    b := u8(min(1, 2))
    fmt.println(b)
}
"#,
    );
    assert!(
        go.contains("b := uint8(min(1, 2))"),
        "expected local closure `min` inside `u8(min(1, 2))` to stay a call so Go does not fail with `declared and not used: min`:\n{go}"
    );
}
