use hy_diag::{assert_severity_code_coherence, Pos, Registry, Severity, Span};

#[test]
fn diag_coherence_lookup_and_span_merge_3024() {
    assert_severity_code_coherence("E8001", Severity::Warning);
    assert!(Registry::global().lookup("HYB-HYB-E3003").is_none());
    assert!(Registry::global().lookup("USE-AFTER-MOVE").is_some());
    let s = Span::new(Pos::new(5, 1), Pos::new(5, 8)).to(Span::new(Pos::new(2, 4), Pos::new(2, 10)));
    assert_eq!((s.start, s.end), (Pos::new(2, 4), Pos::new(5, 8)));
}