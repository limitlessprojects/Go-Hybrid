//! LX7 hex/octal string escapes denote bytes, while LX6 rune escapes
//! denote code points. T3 requires each literal string segment to be UTF-8.

use hy_syntax::{lex, Tok};

fn chunks(source: &str) -> Vec<String> {
    lex("byte-escapes.hy", source)
        .unwrap_or_else(|errors| panic!("{source}: {errors:?}"))
        .into_iter()
        .filter_map(|token| match token.tok {
            Tok::StrChunk(text) => Some(text),
            _ => None,
        })
        .collect()
}

#[test]
fn hex_and_octal_escapes_preserve_utf8_bytes() {
    for (source, expected) in [
        (r#""\xc3\xa9""#, "é"),
        (r#""\303\251""#, "é"),
        (r#""\xc3\251""#, "é"),
        (r#""\xe4\xb8\xad""#, "中"),
        (r#""\xf0\x9f\x99\x82""#, "🙂"),
        (r#""e\xcc\x81""#, "e\u{301}"),
        (r#""\x00é\177\xc2\x80""#, "\0é\u{7f}\u{80}"),
        (r#""\1012\0\000\012\xc3\xbf""#, "A2\0\0\nÿ"),
    ] {
        assert_eq!(chunks(source), [expected], "{source}");
    }
}

#[test]
fn utf8_boundary_scalars_round_trip_in_both_byte_escape_forms() {
    for codepoint in [
        0, 0x7f, 0x80, 0x7ff, 0x800, 0xd7ff, 0xe000, 0xffff, 0x10000, 0x3ffff, 0x40000, 0xfffff,
        0x100000, 0x10ffff,
    ] {
        let expected = char::from_u32(codepoint).unwrap().to_string();
        let hex: String = expected.bytes().map(|b| format!("\\x{b:02x}")).collect();
        let octal: String = expected.bytes().map(|b| format!("\\{b:03o}")).collect();
        for escaped in [hex, octal] {
            assert_eq!(chunks(&format!("\"{escaped}\"")), [expected.as_str()]);
        }
    }
}

#[test]
fn interpolation_and_nested_strings_decode_independent_chunks() {
    assert_eq!(chunks(r#""\xc3\xa9{value}\344\270\255""#), ["é", "中"]);
    assert_eq!(
        chunks(r#""\xc3\xa9{"\xe4\xb8\xad"}\xf0\x9f\x99\x82""#),
        ["é", "中", "🙂"]
    );
    assert_eq!(chunks(r#""{{\xc3\xa9}}""#), ["{é}"]);
}

#[test]
fn unicode_escapes_raw_strings_and_runes_keep_their_semantics() {
    assert_eq!(chunks(r#""\u00e9\U0001f642\x41\n\\\"""#), ["é🙂A\n\\\""]);
    let tokens = lex("controls.hy", r#"'\xe9' '\377' '\u{e9}' `\xc3\xa9`"#)
        .expect("valid runes and raw string");
    let values: Vec<_> = tokens.into_iter().map(|t| t.tok).collect();
    assert_eq!(
        values,
        [
            Tok::CharLit('é'),
            Tok::CharLit('ÿ'),
            Tok::CharLit('é'),
            Tok::RawStr("\\xc3\\xa9".into()),
            Tok::Eof,
        ]
    );
}

#[test]
fn invalid_utf8_bytes_are_diagnosed_instead_of_reencoded_or_replaced() {
    for source in [
        r#""\xff""#,
        r#""\377""#,
        r#""\x80""#,
        r#""\xc3""#,
        r#""\xc3A""#,
        r#""\xc0\xaf""#,
        r#""\xe0\x80\xaf""#,
        r#""\xed\xa0\x80""#,
        r#""\xf4\x90\x80\x80""#,
        r#""\xf5\x80\x80\x80""#,
        r#""\xf0\x9f\x99""#,
        r#""\xc3\u00a9""#,
        r#""\xc3{value}\xa9""#,
        r#""{value}\xa9""#,
    ] {
        let errors = lex("invalid-bytes.hy", source).expect_err(source);
        assert!(
            errors.iter().any(|d| d.code == "HYB-E0002"
                && d.tier0_rule.as_deref() == Some("LX7")
                && d.message.contains("invalid UTF-8")),
            "{source}: {errors:?}"
        );
    }
}
