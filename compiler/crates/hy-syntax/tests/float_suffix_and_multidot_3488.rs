use hy_syntax::{lex, parse_file};

#[test]
fn non_exponent_float_suffixes_and_triple_dots_fail_closed_3488() {
    for src in ["fn main() { f := 1\n x := 1.5f }", "fn main() { f64 := 1\n x := 1.5f64 }", "fn main() { f := 1\n x := .5f }", "fn main() { x := 1.2.3.4 }"] {
        let errs = lex("test.hy", src).expect_err("malformed float must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}