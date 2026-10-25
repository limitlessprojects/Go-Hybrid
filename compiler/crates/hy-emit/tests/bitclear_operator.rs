//! `&^` (Go's AND NOT) must survive parsing and emission as `&^`.
//!
//! It used to lex as `&` then `^` and fail at parse time. This guards the whole
//! chain: lex, parse at the MulOp precedence, and emit.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn bitclear_binary_and_compound_emit_their_operators() {
    let go = emit_src(
        "\
fn main() {
    a := 6 &^ 3
    a &^= 1
    _ = a
}
",
    );
    assert!(go.contains("&^"), "expected `&^` in output, got:\n{go}");
    assert!(go.contains("&^="), "expected `&^=` in output, got:\n{go}");
}

#[test]
fn bitclear_binds_at_mulop_precedence() {
    // `1 + 2 &^ 3` is `1 + (2 &^ 3)`, so the `+` must stay outside.
    let go = emit_src(
        "\
fn main() {
    x := 1 + 2 &^ 3
    _ = x
}
",
    );
    assert!(
        go.contains("1 + 2 &^ 3"),
        "expected `&^` to bind tighter than `+`, got:\n{go}"
    );
}
