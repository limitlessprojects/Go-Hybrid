use hy_syntax::{lex, parse_file};

#[test]
fn integer_trailing_alphabetic_characters_fail_closed_3498() {
    let malformed = [
        "fn main() { x := 42abc }",
        "fn main() { x := 100u64 }",
        "fn main() { x := 100i32 }",
        "fn main() { x := 100f }",
        "fn main() { x := 100i }",
        "fn main() { x := 100u }",
        "fn main() { x := 0abc }",
        "fn main() { x := 077a }",
        "fn main() { x := 99xyz }",
        "fn main() { abc := 1\n x := 42abc }",
        "fn main() { x := 42abc + 1 }",
    ];

    for src in malformed {
        let errs = lex("test.hy", src)
            .expect_err(&format!("malformed integer literal must fail lexing: {src}"));
        assert!(
            errs.iter().any(|d| {
                d.code == "HYB-E0002"
                    && d.tier0_rule.as_deref() == Some("LX6")
                    && d.message.contains("trailing alphabetic characters")
            }),
            "expected HYB-E0002 [LX6] trailing alphabetic diagnostic for `{src}`, got: {errs:?}"
        );
        assert!(
            parse_file("test.hy", src).is_err(),
            "expected parse_file to fail for `{src}`"
        );
    }
}

#[test]
fn valid_integer_expressions_remain_accepted_3498() {
    let valid = [
        "fn main() { for i in 0..10 {} }",
        "fn main() { for j in 1..=8 {} }",
        "fn main() { x := 42 + 1 }",
        "fn main() { x := 42 }",
        "fn main() { x := 0 }",
        "fn main() { x := 0755 }",
    ];

    for src in valid {
        assert!(
            lex("test.hy", src).is_ok(),
            "expected lex to accept valid syntax `{src}`"
        );
        assert!(
            parse_file("test.hy", src).is_ok(),
            "expected parse_file to accept valid syntax `{src}`"
        );
    }
}
