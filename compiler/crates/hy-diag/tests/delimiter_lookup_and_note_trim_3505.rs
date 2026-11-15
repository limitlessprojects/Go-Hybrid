use hy_diag::{Diagnostic, Pos, Registry, Severity, Span, assert_severity_code_coherence};

#[test]
fn delimiter_wrapped_lookup_and_note_trim_3505() {
    let reg = Registry::global();
    for q in ["[HYB-E3003]", "[HYB-E3003]:", "`E3003`", "\"use-after-move\"", "'3003'"] {
        assert_eq!(reg.lookup(q).map(|e| e.code.as_str()), Some("HYB-E3003"));
    }
    assert_severity_code_coherence("[HYB-E8001]:", Severity::Warning);
    let d = Diagnostic::error("[hyb_e0999]:", "t.hy", Span::point(Pos::new(1, 1)), "msg").with_notes(vec!["  first note  ".into(), " ".into()]);
    assert_eq!((d.code.as_str(), d.notes.as_slice()), ("HYB-E0999", &["first note".to_string()][..]));
}