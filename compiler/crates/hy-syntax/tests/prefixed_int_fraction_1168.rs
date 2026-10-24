use hy_syntax::{Tok, lex, parse_file};

fn lex_errs(src: &str) -> Vec<hy_diag::Diagnostic> {
    match lex("test.hy", src) {
        Ok(toks) => panic!("expected lex failure for `{src}`, got tokens: {toks:?}"),
        Err(diags) => diags,
    }
}

#[test]
fn prefixed_integer_fractional_suffix_is_diagnosed_issue_1168() {
    for (src, base) in [
        ("fn main() { n := 0x1.8 }", "0x"),
        ("fn main() { n := 0x1.f }", "0x"),
        ("fn main() { n := 0x1.8p-2 }", "0x"),
        ("fn main() { n := 0x1. }", "0x"),
        ("fn main() { n := 0o7.5 }", "0o"),
        ("fn main() { n := 0b1.0 }", "0b"),
    ] {
        let errs = lex_errs(src);
        assert!(
            errs.iter().any(|d| d.code == "HYB-E0002"
                && d.message.contains(&format!("malformed {base} integer literal"))
                && d.tier0_rule.as_deref() == Some("LX6")),
            "expected HYB-E0002 [LX6] for `{src}`, got: {errs:?}"
        );
    }

    for valid in [
        "fn main() { for i in 0x1..0x10 { } }",
        "fn main() { for i in 0x1..=0x10 { } }",
    ] {
        assert!(parse_file("test.hy", valid).is_ok());
    }
    let toks: Vec<Tok> = lex("test.hy", "0x1..0x10")
        .expect("range lexes")
        .into_iter()
        .map(|s| s.tok)
        .collect();
    assert_eq!(toks[0], Tok::Int("0x1".into()));
    assert_eq!(toks[1], Tok::Punct(".."));
    assert_eq!(toks[2], Tok::Int("0x10".into()));
}
