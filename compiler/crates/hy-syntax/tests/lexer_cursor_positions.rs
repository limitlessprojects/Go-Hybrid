use hy_syntax::{lex, Tok, Token};

fn coordinates(token: &Token) -> (u32, u32, u32, u32) {
    (
        token.span.start.line,
        token.span.start.col,
        token.span.end.line,
        token.span.end.col,
    )
}

#[test]
fn map_trivia_lookahead_keeps_character_positions() {
    for (source, bracket_column) in [
        ("map /* λ🙂 */\n\t[string]int", 2),
        ("map // λ🙂\n[string]int", 1),
    ] {
        let tokens = lex("unicode.hy", source).expect("map prefix after comments");
        assert_eq!(tokens[0].tok, Tok::Ident("map".into()));
        assert_eq!(coordinates(&tokens[0]), (1, 1, 1, 4));
        assert_eq!(tokens[1].tok, Tok::Punct("["));
        assert_eq!(
            coordinates(&tokens[1]),
            (2, bracket_column, 2, bracket_column + 1)
        );
    }
}

#[test]
fn multiline_raw_string_counts_characters_and_has_one_eof() {
    let tokens = lex("unicode.hy", "let message := `λ🙂\ntext`\n").expect("raw string");
    let raw = tokens
        .iter()
        .find(|t| matches!(&t.tok, Tok::RawStr(_)))
        .unwrap();
    assert_eq!(raw.tok, Tok::RawStr("λ🙂\ntext".into()));
    assert_eq!(coordinates(raw), (1, 16, 2, 6));
    let eof = tokens.last().unwrap();
    assert!(matches!(&eof.tok, Tok::Eof));
    assert_eq!(coordinates(eof), (3, 1, 3, 1));
    assert_eq!(
        tokens.iter().filter(|t| matches!(&t.tok, Tok::Eof)).count(),
        1
    );
}
