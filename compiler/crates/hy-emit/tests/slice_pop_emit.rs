use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn emits_hy_pop_for_parenthesized_field_and_index_slice_bases() {
    let go = emit(
        "struct holder {\n\
           stack []int\n\
         }\n\
         fn main() {\n\
           s := []int{1, 2, 3}\n\
           a := (s).pop()\n\
           h := holder{stack: []int{4, 5}}\n\
           b := h.stack.pop()\n\
           grid := [][]int{[]int{6, 7}}\n\
           c := grid[0].pop()\n\
           _ = a + b + c\n\
         }\n",
    );
    assert!(
        go.contains("func hy_pop[t any](s *[]t) t"),
        "expected hy_pop prelude in:\n{go}"
    );
    assert!(
        go.contains("a := hy_pop(&s)"),
        "expected parenthesized slice base `(s).pop()` to lower to `hy_pop(&s)` in:\n{go}"
    );
    assert!(
        go.contains("b := hy_pop(&h.stack)"),
        "expected struct slice field `h.stack.pop()` to lower to `hy_pop(&h.stack)` in:\n{go}"
    );
    assert!(
        go.contains("c := hy_pop(&grid[0])"),
        "expected slice element `grid[0].pop()` to lower to `hy_pop(&grid[0])` in:\n{go}"
    );
}

#[test]
fn does_not_hijack_user_struct_pop_methods() {
    let go = emit(
        "struct custom_stack {\n\
           top int\n\
         }\n\
         fn (c *custom_stack) pop() int {\n\
           return c.top\n\
         }\n\
         fn main() {\n\
           cs := custom_stack{top: 42}\n\
           v := cs.pop()\n\
           _ = v\n\
         }\n",
    );
    assert!(
        go.contains("v := cs.pop()"),
        "expected user struct `.pop()` method call `cs.pop()` to remain `cs.pop()`, got:\n{go}"
    );
    assert!(
        !go.contains("hy_pop("),
        "user struct `.pop()` method must not emit `hy_pop`: \n{go}"
    );
}
