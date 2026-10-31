//! array_len_nil_complex_1810.rs — regression for issue #1810.
//!
//! Go rejects non-integer constants in array lengths:
//! "cannot use nil as integer constant in array length" and the same for
//! complex values. `is_non_int_array_len_expr` only recognised bool, string,
//! float and compound expressions, so `[nil]int` and `[complex64(1)]int`
//! passed `hyb check` (HYB-E6004 is the checker's non-integer length error).

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
        found.iter().any(|code| code == "HYB-E6004" || code == "HYB-E1002"),
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
fn rejects_nil_array_length() {
    assert_reject(
        r#"fn main() {
    var a [nil]int
    _ = a
}
"#,
    );
}

#[test]
fn rejects_complex_array_length() {
    assert_reject(
        r#"fn main() {
    var a [complex64(1)]int
    _ = a
}
"#,
    );
}

#[test]
fn rejects_complex_builtin_array_length() {
    assert_reject(
        r#"fn main() {
    var a [complex(1.0, 2.0)]int
    _ = a
}
"#,
    );
}

#[test]
fn accepts_integer_lengths() {
    assert_accept(
        r#"fn main() {
    var a [3]int
    var b [len("abc")]int
    var c [1 + 2]int
    _ = a
    _ = b
    _ = c
}
"#,
    );
}
