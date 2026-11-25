use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn var_and_const_option_none_emit_zero_struct() {
    let go = emit(
        "fn main() {\n\
             var o option[int] = none\n\
             _ = o\n\
         }\n",
    );
    assert!(
        go.contains("var o option[int] = option[int]{}"),
        "expected `var o option[int] = option[int]{{}}`, got:\n{go}"
    );
}

#[test]
fn struct_field_slice_and_map_option_none_emit_zero_struct() {
    let go = emit(
        "struct cfg { v option[int] }\n\
         fn main() {\n\
             c := cfg{v: none}\n\
             xs := []option[int]{none, some(1)}\n\
             m := map[string]option[int]{\"a\": none}\n\
             _ = c\n\
             _ = xs\n\
             _ = m\n\
         }\n",
    );
    assert!(
        go.contains("cfg{v: option[int]{}}"),
        "expected `cfg{{v: option[int]{{}}}}`, got:\n{go}"
    );
    assert!(
        go.contains("[]option[int]{option[int]{}, some(1)}"),
        "expected `[]option[int]{{option[int]{{}}, some(1)}}`, got:\n{go}"
    );
    assert!(
        go.contains("map[string]option[int]{\"a\": option[int]{}}"),
        "expected `map[string]option[int]{{\"a\": option[int]{{}}}}`, got:\n{go}"
    );
}

#[test]
fn parenthesized_return_none_emits_zero_struct() {
    let go = emit(
        "fn find() option[int] {\n\
             return (none)\n\
         }\n\
         fn main() {\n\
             _ = find()\n\
         }\n",
    );
    assert!(
        go.contains("return option[int]{}"),
        "expected `return option[int]{{}}`, got:\n{go}"
    );
}
