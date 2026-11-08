use hy_diag::{Diagnostic, Pos, Registry, Severity, Span, assert_severity_code_coherence};

#[test]
fn underscore_lookup_and_unregistered_hyb_8nnn_coherence_3487() {
    let reg = Registry::global();
    for q in ["HYB_E3003", "hyb_3003", "use_after_move", "  USE_AFTER_MOVE  "] {
        assert_eq!(reg.lookup(q).map(|e| e.code.as_str()), Some("HYB-E3003"));
    }
    assert_severity_code_coherence("HYB-8999", Severity::Warning);
    let w = Diagnostic::warning("hyb_8999", "t.hy", Span::point(Pos::new(1, 1)), "lint");
    assert_eq!(w.code, "HYB-8999");
}