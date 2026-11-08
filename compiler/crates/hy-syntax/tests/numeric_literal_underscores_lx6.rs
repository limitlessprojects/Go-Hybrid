use hy_syntax::{lex, parse_file};

#[test]
fn numeric_literal_underscores_fail_closed_lx6() {
    let invalid_cases = [
        // Decimal integers
        "fn main() { x := 42_ }",
        "fn main() { x := 1__0 }",
        "fn main() { x := 10__000 }",
        // Prefixed integers (hex, binary, octal)
        "fn main() { x := 0x_1 }",
        "fn main() { x := 0x1__0 }",
        "fn main() { x := 0x1_ }",
        "fn main() { x := 0b_1 }",
        "fn main() { x := 0b1__0 }",
        "fn main() { x := 0b1_ }",
        "fn main() { x := 0o_1 }",
        "fn main() { x := 0o7__7 }",
        "fn main() { x := 0o7_ }",
        // Floating point fractions
        "fn main() { x := 1._5 }",
        "fn main() { x := 1_.5 }",
        "fn main() { x := 1.5__0 }",
        "fn main() { x := 1.5_ }",
        // Floating point exponents
        "fn main() { x := 1e_5 }",
        "fn main() { x := 1e+_5 }",
        "fn main() { x := 1e-_5 }",
        "fn main() { x := 1e5__0 }",
        "fn main() { x := 1e5_ }",
    ];

    for src in invalid_cases {
        let errs = lex("test.hy", src).expect_err(&format!("malformed literal `{src}` must fail lexing"));
        assert!(
            errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")),
            "expected HYB-E0002 with LX6 rule for `{src}`, got: {errs:?}"
        );
        assert!(parse_file("test.hy", src).is_err(), "parse_file must fail for `{src}`");
    }
}

#[test]
fn valid_numeric_literal_underscores_accepted_lx6() {
    let valid_cases = [
        "fn main() { x := 1_000 }",
        "fn main() { x := 1_000_000 }",
        "fn main() { x := 0x1_f }",
        "fn main() { x := 0b1010_0101 }",
        "fn main() { x := 0o7_7_7 }",
        "fn main() { x := 1_000.5_000 }",
        "fn main() { x := 1.5e1_0 }",
        "fn main() { x := 1.5e+1_0 }",
        "fn main() { x := 1.5e-1_0 }",
    ];

    for src in valid_cases {
        let toks = lex("test.hy", src).expect(&format!("valid literal `{src}` must lex successfully"));
        assert!(!toks.is_empty());
        assert!(parse_file("test.hy", src).is_ok(), "valid literal `{src}` must parse cleanly");
    }
}
