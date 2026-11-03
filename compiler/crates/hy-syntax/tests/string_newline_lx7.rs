use hy_syntax::{lex, parse_file};

#[test]
fn unescaped_newline_in_interpreted_string_fails_closed_lx7() {
    let invalid_cases = [
        "fn main() {\n s := \"hello\nworld\"\n}",
        "fn main() {\n s := \"line1\nline2\nline3\"\n}",
        "fn main() {\n s := \"unterminated newline\n\n}",
        "fn main() {\n s := \"prefix {x} middle\nsuffix\"\n}",
    ];

    for src in invalid_cases {
        let errs = lex("test.hy", src).expect_err(&format!("unescaped newline in `{src}` must fail lexing"));
        assert!(
            errs.iter().any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX7")),
            "expected HYB-E0002 with LX7 rule for `{src}`, got: {errs:?}"
        );
        assert!(parse_file("test.hy", src).is_err(), "parse_file must fail for `{src}`");
    }
}

#[test]
fn escaped_newlines_and_raw_strings_accepted_lx7() {
    let valid_cases = [
        "fn main() {\n s := \"hello\\nworld\"\n}",
        "fn main() {\n s := `hello\nworld`\n}",
        "fn main() {\n s := `multiline\nraw\nstring`\n}",
        "fn main() {\n s := \"escaped: \\t \\r \\n \\\" end\"\n}",
    ];

    for src in valid_cases {
        let toks = lex("test.hy", src).expect(&format!("valid string `{src}` must lex successfully"));
        assert!(!toks.is_empty());
        assert!(parse_file("test.hy", src).is_ok(), "valid string `{src}` must parse cleanly");
    }
}
