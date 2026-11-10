use hy_syntax::{lex, parse_file};

#[test]
fn multi_dot_float_literals_fail_closed_3451() {
    for src in ["fn main() { x := 1.2.3 }", "fn main() { x := .5.2 }", "fn main() { x := 1e2.5 }"] {
        let errs = lex("test.hy", src).expect_err("multi-dot float must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}
