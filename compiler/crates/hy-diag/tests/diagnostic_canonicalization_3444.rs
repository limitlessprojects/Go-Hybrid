use hy_diag::{envelope, Diagnostic, Pos, Span};

#[test]
fn lowercase_fallback_lint_code_is_canonical_and_empty_rules_are_omitted() {
    let span = Span::point(Pos::new(1, 1));
    let diagnostic = Diagnostic::warning(" hyb-e8999 ", "test.hy", span, "lint");
    assert_eq!(diagnostic.code, "HYB-E8999");

    let diagnostic = Diagnostic::error("HYB-E3003", "test.hy", span, "error").with_rule("  A7  ");
    assert_eq!(diagnostic.tier0_rule.as_deref(), Some("A7"));

    let diagnostic = Diagnostic::error("HYB-E3003", "test.hy", span, "error").with_rule("   ");
    assert_eq!(diagnostic.tier0_rule, None);
    let json: serde_json::Value =
        serde_json::from_str(&envelope(&[diagnostic])).expect("envelope serializes");
    assert!(json["diagnostics"][0].get("tier0_rule").is_none());
}
