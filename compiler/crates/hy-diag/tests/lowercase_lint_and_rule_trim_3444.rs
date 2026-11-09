use hy_diag::{assert_severity_code_coherence, envelope, Diagnostic, Pos, Severity, Span};

#[test]
fn lowercase_lint_coherence_and_with_rule_trim_3444() {
    let span = Span::point(Pos::new(1, 1));
    assert_severity_code_coherence(" hyb-e8999 ", Severity::Warning);
    let warn = Diagnostic::warning(" hyb-e8999 ", "t.hy", span, "lint").with_rule("   ");
    assert_eq!(warn.code, "HYB-E8999");
    assert_eq!(warn.tier0_rule, None);
    assert!(!envelope(&[warn]).contains("tier0_rule"));

    let err = Diagnostic::error("hyb-e0999", "t.hy", span, "err").with_rule("  LX6  ");
    assert_eq!(err.code, "HYB-E0999");
    assert_eq!(err.tier0_rule.as_deref(), Some("LX6"));
}
