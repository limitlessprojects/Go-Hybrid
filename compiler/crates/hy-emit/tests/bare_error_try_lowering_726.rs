use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("t.hy", src).expect("parse");
    emit_go(&file).expect("emit")
}

#[test]
fn try_bare_error_call_lowering_issue_726() {
    let go = emit(
        r#"package main

fn step() error {
    return nil
}

fn run() error {
    step()?
    return nil
}

fn main() {
    run()?
    fmt.println("ok")
}
"#,
    );
    assert!(
        !go.contains("hyT0, hyErr"),
        "bare error try must not allocate value temp:\n{}",
        go
    );
    assert!(
        go.contains("hyErr0 := step()\n\tif hyErr0 != nil {\n\t\treturn hyErr0\n\t}"),
        "expected direct hyErr assignment for step, got:\n{}",
        go
    );
    assert!(
        go.contains("hyErr0 := run()") && go.contains("hyErr = hyErr0"),
        "expected direct hyErr assignment for run in main, got:\n{}",
        go
    );
}
