use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn zero_arg_assoc_fn_pop_lowers_to_mangled_call_without_hy_pop() {
    let go = emit(
        "import \"fmt\"\n\n\
         struct holder { tag int }\n\n\
         fn holder.pop() bool {\n\
             return true\n\
         }\n\n\
         fn main() {\n\
             ok := holder.pop()\n\
             if ok {\n\
                 fmt.println(\"popped\")\n\
             }\n\
         }\n",
    );
    assert!(
        go.contains("ok := holder_pop()"),
        "expected `ok := holder_pop()`, got:\n{go}"
    );
    assert!(
        !go.contains("hy_pop(") && !go.contains("func hy_pop"),
        "associated fn `holder.pop()` must not emit `hy_pop` or its prelude:\n{go}"
    );
}

#[test]
fn parenthesized_zero_arg_assoc_fn_pop_lowers_to_mangled_call() {
    let go = emit(
        "struct holder { tag int }\n\
         fn holder.pop() bool {\n\
             return true\n\
         }\n\
         fn main() {\n\
             ok := (holder).pop()\n\
             _ = ok\n\
         }\n",
    );
    assert!(
        go.contains("ok := holder_pop()"),
        "expected `(holder).pop()` to lower to `holder_pop()`, got:\n{go}"
    );
    assert!(
        !go.contains("hy_pop("),
        "`(holder).pop()` must not emit `hy_pop`:\n{go}"
    );
}

#[test]
fn local_slice_shadowing_type_name_still_lowers_to_hy_pop() {
    let go = emit(
        "struct holder { tag int }\n\
         fn holder.pop() bool {\n\
             return true\n\
         }\n\
         fn main() {\n\
             holder := []int{1, 2}\n\
             v := holder.pop()\n\
             _ = v\n\
         }\n",
    );
    assert!(
        go.contains("v := hy_pop(&holder)") && go.contains("func hy_pop[t any](s *[]t) t"),
        "local slice shadowing `holder` must still lower to `hy_pop(&holder)`, got:\n{go}"
    );
}

#[test]
fn pointer_receiver_pop_method_still_emits_method_call() {
    let go = emit(
        "struct holder { tag int }\n\
         fn (h *holder) pop() bool {\n\
             return h.tag > 0\n\
         }\n\
         fn main() {\n\
             h := holder{tag: 1}\n\
             ok := h.pop()\n\
             _ = ok\n\
         }\n",
    );
    assert!(
        go.contains("ok := h.pop()"),
        "expected pointer-receiver method `h.pop()` to stay `h.pop()`, got:\n{go}"
    );
    assert!(
        !go.contains("hy_pop("),
        "pointer-receiver `.pop()` must not emit `hy_pop`:\n{go}"
    );
}
