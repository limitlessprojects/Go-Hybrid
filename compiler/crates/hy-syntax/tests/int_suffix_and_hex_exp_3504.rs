use hy_syntax::{lex, parse_file};

#[test]
fn int_trailing_suffixes_and_dotless_hex_exponents_fail_closed_3504() {
    for src in ["fn main() { u8 := 1\n x := 123u8 }", "fn main() { i64 := 1\n x := 42i64 }", "fn main() { a := 1\n x := 07a }", "fn main() { x := 0x1p-2 }", "fn main() { x := 0x1P+2 }"] {
        let errs = lex("test.hy", src).expect_err("malformed integer literal must fail lexing");
        assert!(errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")));
        assert!(parse_file("test.hy", src).is_err());
    }
}