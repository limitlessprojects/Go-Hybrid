//! Issue #2384: `defer f.close()?` and `defer (f.close())?` pass `hyb check`
//! (`check_stmt_defer_call` and `check_defer_arity` peel outer `Expr::Try`)
//! and must emit cleanly without failing with "`?` outside a binding position".

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

#[test]
fn defer_with_outer_try_emits_cleanly() {
    let src = r#"import "os"

fn step() (int, error) {
    return 1, nil
}

fn run(path string) error {
    f := os.open(path)?
    defer f.close()?
    defer (f.close())?
    defer f.seek(int64(step()?), 0)?
    return nil
}
"#;
    let go = emit(src);
    assert!(go.contains("defer f.Close()"), "{go}");
    assert!(go.contains("defer f.Seek("), "{go}");
}
// Row 13 BUILDABLE regression test (#2384)
