//! Regression test for #2442: empty slice `.pop()` panics with `HY-PANIC/pop-empty`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn slice_pop_prelude_panics_with_hy_panic_on_empty() {
    let src = "fn main() {\n\
               \ts := []int{}\n\
               \t_ = s.pop()\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit cleanly");
    assert!(
        go.contains("if n == 0 {\n\t\tpanic(\"HY-PANIC/pop-empty\")\n\t}"),
        "expected HY-PANIC/pop-empty guard in POP_PRELUDE, got:\n{go}"
    );
    assert!(
        go.contains("v := (*s)[n-1]\n\t*s = (*s)[:n-1]\n\treturn v"),
        "expected non-empty slice pop body to remain intact, got:\n{go}"
    );
}
