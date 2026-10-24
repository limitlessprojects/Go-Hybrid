//! Regression tests for issue #723: composite literals (O4.6), `const` value
//! composites (Class 4), and mutable composite bindings (Class 3 via `move`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn fresh_composite_literal_and_const_value_composite_accept_without_move() {
    let src = r#"
struct pair {
    a int
    b int
}

const c_pair = pair{a: 1, b: 2}
const c_arr = [2]int{10, 20}

fn worker(p pair, arr [2]int, out chan<- pair, out_arr chan<- [2]int) {
    out <- pair{a: p.a, b: p.b}
    out_arr <- [2]int{arr[0], arr[1]}
}

fn main() {
    out := make(chan pair, 2)
    out_arr := make(chan [2]int, 2)
    task worker(pair{a: 1, b: 2}, [2]int{3, 4}, out, out_arr)
    task worker(c_pair, c_arr, out, out_arr)
    out <- c_pair
    out_arr <- c_arr
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn mutable_value_composite_binding_requires_move_and_explains_why() {
    let src_spawn = r#"
struct pair {
    a int
    b int
}

fn worker(p pair) {
    _ = p
}

fn main() {
    var p pair
    p.a = 1
    task worker(p)
}
"#;
    let r_spawn = check(src_spawn);
    assert_eq!(r_spawn.verdict, Verdict::Reject);
    assert!(error_codes(&r_spawn).contains(&"HYB-E3001".to_string()));
    let notes_spawn = r_spawn.diagnostics[0].notes.join("
");
    assert!(
        notes_spawn.contains("issue #723"),
        "expected issue #723 explanation note on mutable struct spawn reject, got:
{notes_spawn}"
    );

    let src_send = r#"
fn main() {
    var a [2]int
    a[0] = 1
    c := make(chan [2]int, 1)
    c <- a
}
"#;
    let r_send = check(src_send);
    assert_eq!(r_send.verdict, Verdict::Reject);
    assert!(error_codes(&r_send).contains(&"HYB-E3002".to_string()));
    let notes_send = r_send.diagnostics[0].notes.join("
");
    assert!(
        notes_send.contains("issue #723"),
        "expected issue #723 explanation note on mutable array send reject, got:
{notes_send}"
    );

    let src_moved = r#"
struct pair {
    a int
    b int
}

fn worker(p pair) {
    _ = p
}

fn main() {
    var p pair
    p.a = 1
    task worker(move p)
    var a [2]int
    a[0] = 1
    c := make(chan [2]int, 1)
    c <- move a
}
"#;
    let r_moved = check(src_moved);
    assert_eq!(
        r_moved.verdict,
        Verdict::Accept,
        "expected Accept when mutable value composites are moved, got: {:?}",
        r_moved.diagnostics
    );
}
