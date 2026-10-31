use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn test_return_exact_multi_call_no_redundant_nil() {
    let src = r#"
fn pair() (int, error) {
    return 1, nil
}

fn worker() (int, error) {
    return pair()
}

fn main() {
    v, err := worker()
    _ = v
    _ = err
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("return pair()"),
        "expected `return pair()`, got:\n{go}"
    );
    assert!(
        !go.contains("return pair(), nil"),
        "did NOT expect `return pair(), nil`, got:\n{go}"
    );
}

#[test]
fn test_return_single_call_zero_fills_error_tail_nil() {
    let src = r#"
fn single() int {
    return 42
}

fn worker() (int, error) {
    return single()
}

fn main() {
    v, err := worker()
    _ = v
    _ = err
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("return single(), nil"),
        "expected `return single(), nil`, got:\n{go}"
    );
}
