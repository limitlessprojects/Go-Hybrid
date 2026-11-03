//! #1850 — a unary operand is a single-value position.
//!
//! `check_unary_operand` classified the operand kind but never ran the
//! single-value check that a binary operand already runs, so a void call or a
//! multi-value call under `!`/`-`/`&` passed `hyb check` with 0 diagnostics
//! while the emitted Go failed `go build`:
//!
//!   * `!two_bools()` — `multiple-value two_bools() (value of type (bool, bool))
//!     in single-value context`.
//!   * `-two_ints()` — same shape for the numeric unary.
//!
//! The accept cases are the point of this file: `!flag`, `-n`, `&x` and the
//! legal multi-value sources (`a, b := f()`) must keep working.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_rejected(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] among diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn multi_value_call_under_not_is_rejected() {
    let src = r#"
fn two_bools() (bool, bool) {
    return true, false
}

fn main() {
    b := !two_bools()
    _ = b
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn multi_value_call_under_neg_is_rejected() {
    let src = r#"
fn two_ints() (int, int) {
    return 1, 2
}

fn main() {
    n := -two_ints()
    _ = n
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn void_call_under_not_is_rejected() {
    let src = r#"
fn noop() {
}

fn main() {
    b := !noop()
    _ = b
}
"#;
    assert_rejected(src, "HYB-E1002");
}

#[test]
fn plain_unary_and_legal_multi_value_stay_accepted() {
    let src = r#"
fn two_ints() (int, int) {
    return 1, 2
}

fn main() {
    flag := true
    n := 3
    a := !flag
    b := -n
    c, d := two_ints()
    _ = a
    _ = b
    _ = c
    _ = d
}
"#;
    assert_ok(src);
}
