use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn parenthesizes_multi_result_fn_type_expressions() {
    let go = emit(
        "fn apply(cb fn(x int) int bool) (int, bool) {\n\
           return cb(10)\n\
         }\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("func apply(cb func(x int) (int, bool)) (int, bool)"),
        "expected parenthesized multi-result func type in:\n{go}"
    );
    assert!(
        !go.contains("func(x int) int, bool"),
        "must not emit unparenthesized multi-result func type:\n{go}"
    );
}

#[test]
fn isolates_fnlit_return_and_try_lowering_from_enclosing_function() {
    let go = emit(
        "fn step() (int, error) {\n\
           return 1, nil\n\
         }\n\
         fn main() {\n\
           _ = step()?\n\
           f := fn() (int, error) {\n\
             v := step()?\n\
             _, err := step()\n\
             if err != nil {\n\
               return _, err\n\
             }\n\
             return v, nil\n\
           }\n\
           _, _ = f()\n\
           g := fn() option[int] {\n\
             return none\n\
           }\n\
           _ = g()\n\
         }\n",
    );
    assert!(
        go.contains("return 0, hyErr2"),
        "expected `?` inside `fn() (int, error)` closure to propagate `return 0, hyErr2` instead of outer `hyErr = ...`, got:\n{go}"
    );
    assert!(
        go.contains("return 0, err"),
        "expected `return _, err` inside `fn() (int, error)` closure to expand `_` to `0`, got:\n{go}"
    );
    assert!(
        go.contains("return option[int]{}"),
        "expected `return none` inside `fn() option[int]` closure to lower to `return option[int]{{}}`, got:\n{go}"
    );
}
