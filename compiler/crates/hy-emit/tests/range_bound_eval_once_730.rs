use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("t.hy", src).expect("parse");
    emit_go(&file).expect("emit")
}

#[test]
fn range_bound_eval_once_issue_730() {
    let go = emit(
        r#"package main

fn bound() int {
    return 2
}

fn main() {
    n := 3
    for i in 0..n {
        fmt.println("exclusive {i}")
    }
    for n in 0..n {
        fmt.println("grow {n}")
    }
    for i in 0..bound() {
        fmt.println("call {i}")
    }
}
"#,
    );
    assert!(
        go.contains("for i := 0; i < n; i++"),
        "unshadowed ident upper bound stays direct:\n{}",
        go
    );
    assert!(
        go.contains("hyHi0 := n") && go.contains("for n := 0; n < hyHi0; n++"),
        "shadowed loop var must compare against pre-evaluated bound:\n{}",
        go
    );
    assert!(
        go.contains("hyHi1 := bound()") && go.contains("for i := 0; i < hyHi1; i++"),
        "call bound must evaluate once before loop:\n{}",
        go
    );
}
