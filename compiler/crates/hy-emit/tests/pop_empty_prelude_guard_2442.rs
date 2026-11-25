//! Regression test for #2442: non-empty slice `.pop()` lowering preserves `HY-PANIC/pop-empty` prelude.

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn non_empty_slice_pop_emits_branded_prelude_guard() {
    let src = "fn main() {\n\
               \txs := []string{\"a\", \"b\"}\n\
               \tv := xs.pop()\n\
               \t_ = v\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit cleanly");
    assert!(
        go.contains("panic(\"HY-PANIC/pop-empty\")") && go.contains("hy_pop(&xs)"),
        "expected branded empty-pop guard and hy_pop call in:\n{go}"
    );
}
