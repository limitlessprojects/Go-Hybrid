//! Integration tests for issue #1672 (`&^` and `&^=` bitclear / AND NOT operator).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse must succeed");
    emit_go(&ast).expect("emit must succeed")
}

#[test]
fn bitclear_binary_and_compound_assign_emit_go_issue_1672() {
    let src = r#"
fn main() {
    var x int = 0b1100
    y := x &^ 0b0100
    x &^= 0b1000
    z := u8(300 &^ 256)
    _ = y
    _ = x
    _ = z
}
"#;
    let go = emit_src(src);
    assert!(go.contains("x &^ 0b0100"), "missing `&^` in emitted Go:\n{go}");
    assert!(go.contains("x &^= 0b1000"), "missing `&^=` in emitted Go:\n{go}");
}
