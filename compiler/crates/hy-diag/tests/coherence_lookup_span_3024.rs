use hy_diag::{assert_severity_code_coherence, Diagnostic, Pos, Registry, Severity, Span};

#[test]
fn diag_coherence_lookup_and_span_merge_3024() {
    assert_severity_code_coherence("E8001", Severity::Warning);
    assert!(Registry::global().lookup("HYB-HYB-E3003").is_none());
    assert!(Registry::global().lookup("USE-AFTER-MOVE").is_some());
    let s =
        Span::new(Pos::new(5, 1), Pos::new(5, 8)).to(Span::new(Pos::new(2, 4), Pos::new(2, 10)));
    assert_eq!((s.start, s.end), (Pos::new(2, 4), Pos::new(5, 8)));
}

#[test]
fn underscore_aliases_and_unregistered_lint_warning_3487() {
    let registry = Registry::global();
    assert_eq!(registry.lookup("HYB_E3003").unwrap().code, "HYB-E3003");
    assert_eq!(registry.lookup("hyb_3003").unwrap().code, "HYB-E3003");
    assert_eq!(registry.lookup("use_after_move").unwrap().code, "HYB-E3003");

    let span = Span::point(Pos::new(1, 1));
    let warning = Diagnostic::warning("HYB_8999", "t.hy", span, "lint");
    assert_eq!(warning.code, "HYB-8999");
    assert_severity_code_coherence("HYB-8999", Severity::Warning);
}
