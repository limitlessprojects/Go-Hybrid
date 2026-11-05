use hy_syntax::{lex, parse_file};

#[test]
fn trailing_dot_and_suffixed_float_exponents_fail_closed_3478() {
    for src in ["fn main() { x := 2.e+ }", "fn main() { x := 2.e- }", "fn main() { e3 := 1\n x := 1e2e3 }", "fn main() { f := 1\n x := 1.5e2f }"] {
        let errs = lex("test.hy", src).expect_err("malformed float exponent must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}
