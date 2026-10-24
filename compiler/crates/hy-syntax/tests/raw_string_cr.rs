//! Raw-string values must match Go's carriage-return normalization.

use hy_syntax::{lex, Tok};

fn raw_value(src: &str) -> String {
    lex("raw.hy", src)
        .expect("valid raw literal")
        .into_iter()
        .find_map(|token| match token.tok {
            Tok::RawStr(value) => Some(value),
            _ => None,
        })
        .expect("raw string token")
}

#[test]
fn raw_strings_discard_standalone_and_crlf_carriage_returns() {
    for (src, expected) in [
        ("`a\rb`", "ab"),
        ("`a\r\nb`", "a\nb"),
        ("`\r`", ""),
        ("`a\rb\rc`", "abc"),
        ("`é\r界`", "é界"),
    ] {
        assert_eq!(raw_value(src), expected, "source: {src:?}");
    }
}

#[test]
fn raw_strings_keep_linefeeds_and_literal_escape_text() {
    assert_eq!(raw_value("`a\nb`"), "a\nb");
    assert_eq!(raw_value(r#"`a\rb{c}`"#), r#"a\rb{c}"#);
}

#[test]
fn interpreted_carriage_return_escape_is_not_removed() {
    let tokens = lex("interpreted.hy", r#""a\rb""#).expect("valid escape");
    assert!(tokens
        .iter()
        .any(|token| token.tok == Tok::StrChunk("a\rb".into())));
}

#[test]
fn raw_value_normalization_does_not_shift_source_spans() {
    let tokens = lex("spans.hy", "`a\r\nb`\nnext").expect("valid source");
    let next = tokens
        .iter()
        .find(|token| token.tok == Tok::Ident("next".into()))
        .expect("following identifier");
    assert_eq!(next.span.start.line, 3);
    assert_eq!(next.span.start.col, 1);
    assert_eq!(tokens[0].span.end.line, 2);
    assert_eq!(tokens[0].span.end.col, 3);
}
