//! Regression tests for #1991: parenthesized slice `.pop()` and associated function callees.

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn parenthesized_slice_pop_and_assoc_fn_callees_lower_cleanly() {
    let src = "struct box[t any] {\n\
               \tv t\n\
               }\n\
               fn box.new[t any](v t) box[t] {\n\
               \treturn box[t]{v: v}\n\
               }\n\
               fn main() {\n\
               \txs := []int{1, 2}\n\
               \tv := (xs.pop)()\n\
               \tb := (box.new[int])(v)\n\
               \tc := ((box.new)[int])(v)\n\
               \t_ = b.v + c.v\n\
               }\n";
    let ast = parse_file("test.hy", src).expect("must parse");
    let go = emit_go(&ast).expect("must emit cleanly");
    assert!(
        go.contains("hy_pop(&xs)") && go.contains("func hy_pop[t any](s *[]t) t"),
        "expected parenthesized `(xs.pop)()` to lower to `hy_pop(&xs)` with prelude, got:\n{go}"
    );
    assert!(
        go.contains("b := box_new[int](v)") && go.contains("c := box_new[int](v)"),
        "expected parenthesized `(box.new[int])(v)` and `((box.new)[int])(v)` to lower to `box_new[int](v)`, got:\n{go}"
    );
}
