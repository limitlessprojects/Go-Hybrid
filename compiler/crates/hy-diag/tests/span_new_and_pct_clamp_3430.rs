use hy_diag::{assert_severity_code_coherence, Diagnostic, ManagedReport, Pos, Severity, Span};

#[test]
fn span_new_orders_inverted_endpoints_and_pct_clamps() {
    let s = Span::new(Pos::new(9, 4), Pos::new(2, 7));
    assert_eq!((s.start, s.end), (Pos::new(2, 7), Pos::new(9, 4)));

    assert_severity_code_coherence(" E8999 ", Severity::Warning);
    let d = Diagnostic::warning(" E8999 ", "t.hy", s, "msg");
    assert_eq!(d.code, "E8999");

    let rep = ManagedReport { managed_blocks: 1, sites_via_managed: 5, total_sites: 2, ..Default::default() };
    assert_eq!(rep.pct(), 100.0);
}
