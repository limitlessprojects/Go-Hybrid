//! Pointer dereference: `*p` as a value and `*p = v` as an assignment place.
//!
//! The AST had no deref at all, so `*p` failed at parse. `*p = v` on the line
//! after any operand-ending statement also fused into that statement (`p := &x`
//! then `*p = 7` became `&x * p` plus a dangling `= 7`), so the statement-level
//! newline rule now breaks before a `*`/`&`/`-` that opens a new line.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn deref_read_emits_a_star() {
    let go = emit_src(
        "\
fn main() {
    x := 1
    p := &x
    _ = *p
}
",
    );
    assert!(go.contains("_ = *p"), "expected a deref read, got:\n{go}");
}

#[test]
fn deref_assignment_target_emits_parenthesised_deref() {
    let go = emit_src(
        "\
fn main() {
    x := 1
    p := &x
    *p = 7
}
",
    );
    assert!(
        go.contains("(*p) = 7"),
        "expected a parenthesised deref target, got:\n{go}"
    );
    // The `p := &x` statement must stay its own statement, not fuse with `*p`.
    assert!(
        !go.contains("&x * p"),
        "`p := &x` fused with the next line's `*p`, got:\n{go}"
    );
}

#[test]
fn deref_field_target_keeps_precedence() {
    let go = emit_src(
        "\
struct box { n int }

fn main() {
    b := box{n: 1}
    p := &b
    (*p).n = 5
}
",
    );
    assert!(
        go.contains("(*p).n = 5"),
        "`(*p).n` needs the parens to bind correctly, got:\n{go}"
    );
}
