//! Regression tests for #1989 cause 5: pointer-parameter auto-addressing must
//! see through parentheses on both the callee and the argument.

use hy_emit::emit_go;
use hy_syntax::parse_file;

const SRC: &str = "struct env {\n    v int\n}\nfn bump(e *env) {\n    e.v = 1\n}\nfn main() {\n    x := env{}\n";

fn emit(body: &str) -> String {
    let src = format!("{SRC}{body}\n}}\n");
    let ast = parse_file("test.hy", &src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn parenthesized_argument_auto_addresses() {
    let go = emit("    bump((x))");
    assert!(go.contains("bump(&x)"), "{go}");
    assert!(!go.contains("&(x)"), "{go}");
}

#[test]
fn parenthesized_callee_finds_pointer_params() {
    let go = emit("    (bump)(x)");
    assert!(go.contains("&x"), "{go}");
}
