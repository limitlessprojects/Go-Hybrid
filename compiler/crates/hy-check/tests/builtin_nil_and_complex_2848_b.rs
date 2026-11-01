fn codes(src: &str) -> Vec<String> {
    let f = hy_syntax::parse_file("test.hy", src).expect("parse");
    hy_check::check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn rejects_non_numeric_literals_in_real_imag_complex() {
    for src in [
        r#"fn main() { _ = real("abc") }"#,
        "fn main() { _ = imag(true) }",
        "fn main() { _ = real(nil) }",
        r#"fn main() { _ = complex("a", "b") }"#,
        "fn main() { _ = complex(true, 1.0) }",
        "fn main() { _ = complex(1.0, nil) }",
    ] {
        assert!(codes(src).iter().any(|c| c == "HYB-E2005"), "expected HYB-E2005 for: {src}");
    }
    assert!(codes("fn main() { _ = complex(1.0, 2.0) }").is_empty());
}
