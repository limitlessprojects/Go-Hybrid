use hy_syntax::{lex, parse_file};

#[test]
fn numeric_literal_underscores_fail_closed_3553() {
    let invalid_cases = [
        // Decimal integers: consecutive and trailing underscores
        ("1__0", "fn main() { x := 1__0 }"),
        ("42_", "fn main() { x := 42_ }"),
        ("0__755", "fn main() { x := 0__755 }"),
        ("0755_", "fn main() { x := 0755_ }"),
        // Hexadecimal integers: prefix-adjacent, consecutive, and trailing underscores
        ("0x_1", "fn main() { x := 0x_1 }"),
        ("0x1__0", "fn main() { x := 0x1__0 }"),
        ("0x1_", "fn main() { x := 0x1_ }"),
        ("0x_beef", "fn main() { x := 0x_beef }"),
        ("0xdead__beef", "fn main() { x := 0xdead__beef }"),
        ("0xdead_beef_", "fn main() { x := 0xdead_beef_ }"),
        // Binary integers: prefix-adjacent, consecutive, and trailing underscores
        ("0b_1", "fn main() { x := 0b_1 }"),
        ("0b1__0", "fn main() { x := 0b1__0 }"),
        ("0b1_", "fn main() { x := 0b1_ }"),
        // Octal (0o) integers: prefix-adjacent, consecutive, and trailing underscores
        ("0o_1", "fn main() { x := 0o_1 }"),
        ("0o1__0", "fn main() { x := 0o1__0 }"),
        ("0o1_", "fn main() { x := 0o1_ }"),
        // Floating point: underscore adjacent to dot or consecutive in fraction
        ("1._5", "fn main() { x := 1._5 }"),
        ("1_.5", "fn main() { x := 1_.5 }"),
        ("1.5_", "fn main() { x := 1.5_ }"),
        ("1.0__5", "fn main() { x := 1.0__5 }"),
        ("2._", "fn main() { x := 2._ }"),
        (".5_", "fn main() { x := .5_ }"),
        (".5__0", "fn main() { x := .5__0 }"),
        // Floating point exponent: underscore adjacent to e, after sign, or trailing
        ("1_e5", "fn main() { x := 1_e5 }"),
        ("1.5_e2", "fn main() { x := 1.5_e2 }"),
        ("1e_5", "fn main() { x := 1e_5 }"),
        ("1e+_5", "fn main() { x := 1e+_5 }"),
        ("1e-_5", "fn main() { x := 1e-_5 }"),
        ("1e1__0", "fn main() { x := 1e1__0 }"),
        ("1e1_", "fn main() { x := 1e1_ }"),
        ("1.5e1_", "fn main() { x := 1.5e1_ }"),
        // The exact repro from issue #3553
        (
            "repro_3553",
            "fn main() {\n    a := 1__0\n    b := 42_\n    c := 0x_1\n    d := 0x1__0\n    e := 1._5\n}",
        ),
    ];

    for (label, src) in invalid_cases {
        let errs = lex("test.hy", src).expect_err(&format!(
            "malformed numeric literal must fail lexing: {}",
            label
        ));
        assert!(
            errs.iter()
                .any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX6")),
            "expected HYB-E0002 LX6 diagnostic for {}, got: {:?}",
            label,
            errs
        );
        assert!(
            parse_file("test.hy", src).is_err(),
            "parse_file must fail for {}",
            label
        );
    }

    // Valid literals must continue to parse cleanly with 0 diagnostics
    let valid_cases = [
        "fn main() { x := 1_000_000 }",
        "fn main() { x := 42 }",
        "fn main() { x := 0xDEAD_BEEF }",
        "fn main() { x := 0b1010_0101 }",
        "fn main() { x := 0o755 }",
        "fn main() { x := 0o7_55 }",
        "fn main() { x := 0_755 }",
        "fn main() { x := 1.5 }",
        "fn main() { x := 1.0_5 }",
        "fn main() { x := 1_0.0_1 }",
        "fn main() { x := 1e1_0 }",
        "fn main() { x := 1.5e1_0 }",
        "fn main() { x := 2. }",
        "fn main() { x := .5 }",
    ];

    for src in valid_cases {
        let toks = lex("test.hy", src).expect(&format!("valid literal must lex ok: {}", src));
        assert!(
            !toks.is_empty(),
            "valid literal tokens should not be empty: {}",
            src
        );
        let parse_res = parse_file("test.hy", src);
        assert!(
            parse_res.is_ok(),
            "valid literal must parse cleanly: {}, got: {:?}",
            src,
            parse_res.err()
        );
    }
}
