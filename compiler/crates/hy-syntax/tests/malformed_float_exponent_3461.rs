use hy_syntax::{lex, parse_file};

#[test]
fn float_exponent_without_digits_fails_closed_3461() {
    for src in ["fn main() { e := 1\n x := 1.5e }", "fn main() { x := 1.5e+ }", "fn main() { x := 1e- }", "fn main() { e_2 := 1\n x := 1.5e_2 }"] {
        let errs = lex("test.hy", src).expect_err("digitless exponent must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}
