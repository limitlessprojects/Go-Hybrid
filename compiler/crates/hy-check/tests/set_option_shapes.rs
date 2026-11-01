//! set_option_shapes.rs — regression test for #884: the T9 prelude `set[T]` is a
//! struct over a Go map, so a shallow copy shares its storage exactly like any
//! other map-carrying struct. Before this, `set[int]` resolved to Unknown and
//! `shape_may_share` said no, so O5.2/O5.4/T22 could be bypassed by copying
//! first and moving the original.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn copying_a_set_before_moving_the_original_is_rejected() {
    let src = r#"
fn set_worker(s set[int], done chan int) {
    _ = s
    done <- 1
}

fn main() {
    done := make(chan int, 1)
    s1 := set[int]{}
    s1.insert(1)
    s2 := s1
    task set_worker(move s1, done)
    s2.insert(3)
    <-done
}
"#;
    let got = codes(&check(src));
    assert!(
        !got.is_empty(),
        "a set copy shares the underlying map; the move must freeze s2, got {:?}",
        got
    );
}

#[test]
fn mutating_a_const_set_is_rejected() {
    let src = r#"
fn main() {
    const_set.insert(9)
    delete(const_set, 9)
}

const const_set = set[int]{}
"#;
    let got = codes(&check(src));
    assert!(
        !got.is_empty(),
        "T22: a const set must not be mutated, got {:?}",
        got
    );
}
