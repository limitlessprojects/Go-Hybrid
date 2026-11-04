use hy_syntax::{lex, parse_file};

#[test]
fn numeric_underscore_adjacency_fails_closed_3553() {
    // Illegal: underscore adjacency violations must fail lexing with HYB-E0002 [LX6]
    for src in [
        "fn main() { x := 1__0 }",
        "fn main() { x := 42_ }",
        "fn main() { x := 0x_1 }",
        "fn main() { x := 0x1__0 }",
        "fn main() { x := 1._5 }",
        "fn main() { x := 1e_2 }",
        "fn main() { x := 1e__2 }",
        "fn main() { x := 1.5_ }",
    ] {
        let errs = lex("test.hy", src).expect_err("underscore adjacency must fail lexing");
        assert!(
            errs.iter()
                .any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")),
            "expected HYB-E0002 [LX6] for `{src}`, got: {errs:?}"
        );
        assert!(
            parse_file("test.hy", src).is_err(),
            "parse_file must fail for `{src}`"
        );
    }

    // Legal: underscore-separated digits and valid literals must parse cleanly
    for valid in [
        "fn main() { x := 1_000_000 }",
        "fn main() { x := 0xDEAD_BEEF }",
        "fn main() { x := 0b1010_0101 }",
        "fn main() { x := 0755 }",
        "fn main() { x := 2. }",
        "fn main() { x := .5 }",
        "fn main() { x := 1.5 }",
        "fn main() { x := 1e9 }",
        "fn main() { for i in 0..n { } }",
        "fn main() { for i in 1..=8 { } }",
        "fn main() { x := 42.foo() }",
    ] {
        assert!(
            parse_file("test.hy", valid).is_ok(),
            "expected valid numeric literal to parse cleanly: {valid}"
        );
    }
}
