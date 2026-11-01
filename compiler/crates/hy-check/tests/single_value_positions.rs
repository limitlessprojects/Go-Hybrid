//! single_value_positions.rs — regression test for #887: a spawn argument and a
//! channel send are single-value positions. A void call or a multi-value call
//! in either place used to pass `hyb check` and then fail `go build`
//! (`(no value) used as value` / `assignment mismatch`).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn a_void_call_as_a_spawn_argument_is_rejected() {
    let src = r#"
fn noop() {
}

fn worker(x int) {
    _ = x
}

fn main() {
    task worker(noop())
}
"#;
    let got = errors(&check(src));
    assert!(
        !got.is_empty(),
        "a void call cannot be a spawn argument, got {:?}",
        got
    );
}

#[test]
fn a_void_call_in_a_channel_send_is_rejected() {
    let src = r#"
fn noop() {
}

fn main() {
    ch := make(chan int, 1)
    ch <- noop()
}
"#;
    let got = errors(&check(src));
    assert!(
        !got.is_empty(),
        "a void call cannot be sent, got {:?}",
        got
    );
}

#[test]
fn plain_spawn_and_send_stay_accepted() {
    let src = r#"
fn worker(x int) {
    _ = x
}

fn main() {
    ch := make(chan int, 1)
    v := 1
    task worker(v)
    ch <- v
}
"#;
    let got = errors(&check(src));
    assert!(got.is_empty(), "the valid program must stay accepted, got {:?}", got);
}
