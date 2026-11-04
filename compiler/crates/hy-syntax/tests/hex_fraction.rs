//! hex_fraction.rs — regression tests for #1168: `0x1.8` must be one
//! malformed literal, not two statements.

use hy_syntax::parse_file;

#[test]
fn hex_fraction_is_a_malformed_literal() {
    let err = parse_file("hex.hy", "fn main() {\n    n := 0x1.8\n    _ = n\n}\n")
        .expect_err("0x1.8 must not parse as two statements");
    let joined = format!("{err:?}");
    assert!(
        joined.contains("malformed 0x literal"),
        "expected a malformed-prefixed-literal diagnostic, got {joined}"
    );
}

#[test]
fn plain_hex_integer_still_parses() {
    parse_file("hex.hy", "fn main() {\n    n := 0x1F\n    _ = n\n}\n")
        .expect("0x1F is a valid hex integer");
}

#[test]
fn decimal_float_still_parses() {
    parse_file("hex.hy", "fn main() {\n    n := 1.8\n    _ = n\n}\n")
        .expect("1.8 is a valid decimal float");
}
