//! Integration tests for `hy-syntax` lexer Unicode escapes, rune hex/Unicode
//! escapes, delimiter/EOF recovery, and UTF-8 trivia safety:
//! 1. `\UXXXXXXXX` 8-digit and `\uNNNN` 4-digit Unicode escapes in string
//!    literals and rune literals, plus truncated/delimiter-adjacent escapes
//!    (`"\x4"`, `"\u004"`, `"\U0001F60"`, `"\z"`, `'\x4'`, `'\U0001F60'`, `'\\` at EOF)
//!    that must not consume closing quotes or silently pass at EOF.
//! 2. 4-digit `\uNNNN` (`'\u0041'`, `'\u00e9'`, `'\u4e2d'`) and 8-digit
//!    `\UNNNNNNNN` (`'\U0001F600'`) Unicode escapes in rune literals alongside
//!    `\xNN` and `\u{...}`, with fail-closed surrogate/overflow rejection.
//! 3. UTF-8 BOM (`\u{FEFF}`) trivia skipping in `skip_trivia` and
//!    `next_code_char_after_trivia` (including `map /* ... */ \u{feff} [K]V`),
//!    and multi-byte UTF-8 block comments preserving accurate token spans.

use hy_syntax::{Tok, lex, parse_file};

fn lex_errs(src: &str) -> Vec<hy_diag::Diagnostic> {
    match lex("test.hy", src) {
        Ok(toks) => panic!("expected lex failure for `{src}`, got tokens: {toks:?}"),
        Err(diags) => diags,
    }
}

#[test]
fn string_u8_and_truncated_escapes_do_not_consume_closing_quote() {
    let toks = lex(
        "test.hy",
        r#"fn main() { s := "\U0001F600\U00000041\u00e9\x42" }"#,
    )
    .expect("valid string escapes must lex cleanly");
    assert!(
        toks.iter()
            .any(|t| t.tok == Tok::StrChunk("\u{1F600}A\u{e9}B".into())),
        "expected decoded \\U / \\u / \\x string chunk, got: {toks:?}"
    );

    // Truncated or unknown escapes immediately before closing `"` must not
    // consume the closing quote and trigger a cascading `unterminated string literal`.
    for src in [
        r#"fn main() { a := "\x4" b := "ok" }"#,
        r#"fn main() { a := "\u004" b := "ok" }"#,
        r#"fn main() { a := "\U0001F60" b := "ok" }"#,
        r#"fn main() { a := "\z" b := "ok" }"#,
    ] {
        let errs = lex_errs(src);
        assert_eq!(
            errs.len(),
            1,
            "malformed escape before closing quote must not desync quotes on `{src}`: {errs:?}"
        );
        assert!(
            !errs
                .iter()
                .any(|d| d.message.contains("unterminated string literal")),
            "closing quote must not be consumed by malformed escape on `{src}`: {errs:?}"
        );
    }
}

#[test]
fn rune_hex_and_unicode_escapes_and_eof_fail_closed() {
    let toks = lex(
        "test.hy",
        r#"fn main() { a := '\x41' b := '\u00e9' c := '\u4e2d' d := '\U0001F600' e := '\u{1F600}' }"#,
    )
    .expect("valid \\x, \\uNNNN, \\UNNNNNNNN, and \\u{...} rune escapes must lex");
    let chars: Vec<char> = toks
        .into_iter()
        .filter_map(|t| match t.tok {
            Tok::CharLit(ch) => Some(ch),
            _ => None,
        })
        .collect();
    assert_eq!(chars, vec!['A', '\u{e9}', '\u{4e2d}', '\u{1F600}', '\u{1F600}']);

    // Surrogate code points in 4-digit `\uNNNN` and 8-digit `\UNNNNNNNN` rune literals must fail closed.
    for bad_rune in [
        r#"fn main() { c := '\ud800' }"#,
        r#"fn main() { c := '\U0000DFFF' }"#,
        r#"fn main() { c := '\U00110000' }"#,
    ] {
        let errs = lex_errs(bad_rune);
        assert!(
            errs.iter().any(|d| d.code == "HYB-E0002"
                && d.tier0_rule.as_deref() == Some("LX6")
                && d.message.contains("not a Unicode scalar value")),
            "expected HYB-E0002 [LX6] scalar diagnostic for `{bad_rune}`, got: {errs:?}"
        );
    }

    // Truncated escapes before closing `'` must not consume the closing quote of the rune literal.
    for same_line in [
        "fn main() { a := '\\x4' b := 'x' }",
        "fn main() { a := '\\u12' b := 'x' }",
        "fn main() { a := '\\U0001F60' b := 'x' }",
    ] {
        let errs = lex_errs(same_line);
        assert_eq!(
            errs.len(),
            1,
            "truncated rune escape must not eat closing quote and corrupt subsequent `'x'` on `{same_line}`: {errs:?}"
        );
    }

    // Unterminated char literals ending right after `\` or mid-escape at EOF must fail closed.
    for eof_src in [
        "fn main() { c := '\\",
        "fn main() { c := '\\x4",
        "fn main() { c := '\\u00",
        "fn main() { c := '\\U0001",
    ] {
        let errs = lex_errs(eof_src);
        assert!(
            !errs.is_empty(),
            "unterminated char literal at EOF must emit a diagnostic for `{eof_src}`"
        );
    }
}

#[test]
fn utf8_bom_trivia_and_multibyte_block_comments_preserve_lexing_and_spans() {
    let src = "\u{feff}package main\n/* Multi-byte UTF-8 comment: こんにちは 🚀 αβγ */\nfn main() {\n    m := map /* 🦀 */ \u{feff} [string]int{}\n    x := 42\n}\n";
    let parsed = parse_file("test.hy", src);
    assert!(
        parsed.is_ok(),
        "source with leading/inline UTF-8 BOM and multi-byte block comments must parse cleanly: {parsed:?}"
    );

    let toks = lex("test.hy", src).expect("lex ok");
    let assign = toks
        .iter()
        .find(|t| matches!(&t.tok, Tok::Int(s) if s == "42"))
        .expect("found 42 token");
    assert_eq!(assign.span.start.line, 5);
    assert_eq!(assign.span.start.col, 10);
}
