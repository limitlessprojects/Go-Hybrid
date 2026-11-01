use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn typed_let_local_decls_emit_valid_typed_go_vars() {
    let src = r#"
struct stats { count int }
fn (s *stats) bump() { s.count = s.count + 1 }
fn main() {
    var s stats
    let n int64 = 42
    let err error = nil
    let p *stats = &s
    p.bump()
    _ = n
    _ = err
}
"#;
    let file = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&file).expect("typed let declarations must emit valid Go");
    assert!(go.contains("var n int64 = 42"), "got:\n{go}");
    assert!(go.contains("var err error = nil"), "got:\n{go}");
    assert!(go.contains("var p *stats = &s"), "got:\n{go}");
}
