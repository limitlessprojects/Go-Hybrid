use hy_syntax::{lex, parse_file};

#[test]
fn float_exponent_without_digits_fails_closed_3461() {
    for src in ["fn main() { e := 1\n x := 1.5e }", "fn main() { x := 1.5e+ }", "fn main() { x := 1e- }", "fn main() { e_2 := 1\n x := 1.5e_2 }"] {
        let errs = lex("test.hy", src).expect_err("digitless exponent must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}

#[test]
fn malformed_float_exponents_reject_digitless_trailing_dot_and_suffixes_3478() {
    for src in [
        "fn main() { x := 2.e+ }",
        "fn main() { x := 2.e- }",
        "fn main() { x := 1e2e3 }",
        "fn main() { x := 1.5e2f }",
    ] {
        let errs = lex("test.hy", src).expect_err("malformed exponent must fail lexing");
        assert!(
            errs.iter()
                .any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")),
            "expected HYB-E0002 [LX6] for `{src}`, got: {errs:?}"
        );
        assert!(parse_file("test.hy", src).is_err());
    }
}
