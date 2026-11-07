use hy_syntax::parse_file;

#[test]
fn test_literal_after_complete_statement_on_same_line_rejects_3643() {
    let cases = [
        ("fn main() {\n    x := \"a\" \"b\"\n    fmt.println(x)\n}", "adjacent string literal"),
        ("fn main() {\n    x := 1 2\n    fmt.println(x)\n}", "adjacent int literal"),
        ("fn main() {\n    y := x 2\n    fmt.println(y)\n}", "literal after ident value"),
        ("fn main() {\n    fmt.println(1) \"b\"\n}", "literal after call"),
    ];

    for (src, label) in cases {
        let r = parse_file("repro.hy", src);
        assert!(
            r.is_err(),
            "case {label} must reject with parse error, but parsed successfully: {:?}",
            r
        );
        let errs = r.unwrap_err();
        assert!(
            errs.iter().any(|d| d.code == "HYB-E0002"),
            "case {label} must emit HYB-E0002: {:?}",
            errs
        );
    }

    // Negative control: valid newline-separated statements must accept cleanly
    let valid_src = "fn main() {\n    x := \"a\"\n    fmt.println(x)\n}";
    let r = parse_file("valid.hy", valid_src);
    assert!(r.is_ok(), "valid newline-separated code must parse cleanly: {:?}", r.err());
}
