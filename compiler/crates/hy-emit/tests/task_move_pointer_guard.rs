use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("task_move_pointer_guard.hy", src).expect("must parse");
    emit_go(&file).expect("must emit")
}

#[test]
fn moved_pointer_task_arg_is_not_double_addressed() {
    let go = emit(
        r#"
struct env {
    n int
}

fn worker(p *env) {}

fn main() {
    e := env{n: 1}
    p := &e
    task worker(move p)
}
"#,
    );

    assert!(go.contains("go worker(p)"), "moved pointer must stay a *T:\n{}", go);
    assert!(
        !go.contains("go worker(&p)"),
        "task lowering must not turn *T into **T:\n{}",
        go
    );
}

#[test]
fn ordinary_value_task_arg_still_auto_addresses_pointer_param() {
    let go = emit(
        r#"
struct env {
    n int
}

fn worker(p *env) {}

fn main() {
    e := env{n: 1}
    task worker(e)
}
"#,
    );

    assert!(
        go.contains("go worker(&e)"),
        "ordinary addressable T must still auto-address for *T param:\n{}",
        go
    );
}
