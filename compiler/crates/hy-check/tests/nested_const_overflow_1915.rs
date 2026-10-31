//! nested_const_overflow_1915.rs — regression for issue #1915.
//!
//! fixed_width_int_ty_of returned None for unary negation, shifts and binary
//! expressions over typed constants, so check_typed_const_result and
//! check_negated_const_result skipped the folded range check and
//! `-(u8(1) << 1)`, `-i8(127) - 2` and `(u8(1) << 7) + (u8(1) << 7)` passed
//! hyb check while go build rejected them as constant overflow (HYB-E6002).

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
        found.iter().any(|code| code == "HYB-E6002"),
        "expected HYB-E6002, got {found:?} for:\n{src}"
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
fn rejects_negated_shift_overflow() {
    assert_reject(
        r#"fn main() {
    a := -(u8(1) << 1)
    fmt.println(a)
}
"#,
    );
}

#[test]
fn rejects_subtraction_overflow() {
    assert_reject(
        r#"fn main() {
    b := -i8(127) - 2
    fmt.println(b)
}
"#,
    );
}

#[test]
fn rejects_nested_sum_overflow() {
    assert_reject(
        r#"fn main() {
    c := (u8(1) << 7) + (u8(1) << 7)
    fmt.println(c)
}
"#,
    );
}

#[test]
fn accepts_in_range_nested_constants() {
    assert_accept(
        r#"fn main() {
    a := -i8(1)
    b := (u8(1) << 6) + 1
    fmt.println(a)
    fmt.println(b)
}
"#,
    );
}
