use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn typed_let_with_try_preserves_declared_type_annotation() {
    let src = r#"
fn read_val() (int64, error) { return 7, nil }
fn run() (int64, error) {
    let v int64 = read_val()?
    return v, nil
}
"#;
    let file = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&file).expect("typed let with ? must emit valid Go");
    assert!(
        go.contains("var v int64 = hyT0"),
        "expected declared type on `let v int64 = read_val()?` to be preserved, got:\n{go}"
    );
}
