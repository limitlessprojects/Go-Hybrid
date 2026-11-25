//! array_len_builtin_args_2102.rs — regression for issue #2102.
//!
//! `check_array_len` mirrored the constant zero-divisor check onto array
//! length expressions but never ran `check_expr_builtin_args`, so
//! `[len(1)]int` and `[cap(1)+1]int` passed `hyb check` and failed `go build`
//! with "invalid argument ... for built-in len/cap" (Row 13 BUILDABLE).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E2005" || code == "HYB-E1002"),
        "expected an error, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

#[test]
fn rejects_len_of_int_in_array_length() {
    assert_reject(
        r#"fn main() {
    var a [len(1)]int
    _ = a
}
"#,
    );
}

#[test]
fn rejects_cap_of_int_in_array_length() {
    assert_reject(
        r#"fn main() {
    b := [cap(1) + 1]int{0}
    _ = b
}
"#,
    );
}

#[test]
fn rejects_zero_arg_len_in_array_length() {
    assert_reject(
        r#"fn main() {
    var a [len()]int
    _ = a
}
"#,
    );
}

#[test]
fn accepts_valid_len_and_constant_lengths() {
    assert_accept(
        r#"fn main() {
    var a [len("abc")]int
    var b [3]int
    var c [1 + 2]int
    _ = a
    _ = b
    _ = c
}
"#,
    );
}
