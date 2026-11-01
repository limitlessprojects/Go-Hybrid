use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("region_cancellation_test.hy", src).expect("must parse");
    emit_go(&file).expect("must emit")
}

#[test]
fn region_cancellation_prevents_task_execution() {
    let go = emit(
        r#"
region R {
    task worker()
}

fn worker() {}

fn main() {
    R()
}
"#
    );

    assert!(!go.contains("go worker()"), "cancelled region should prevent task execution:\n{}", go);
}

#[test]
fn region_without_cancellation_allows_task_execution() {
    let go = emit(
        r#"
region R {
    task worker()
}

fn worker() {}

fn main() {
    R()
}
"#
    );

    assert!(go.contains("go worker()"), "non-cancelled region should allow task execution:\n{}", go);
}
