use hy_diag::{envelope, Diagnostic, Pos, Registry, Span};

#[test]
fn lookup_accepts_hyb_digits_and_with_notes_filters_blank_3476() {
    let reg = Registry::global();
    assert_eq!(reg.lookup("HYB-3003").map(|e| e.code.as_str()), Some("HYB-E3003"));
    assert_eq!(reg.lookup("hyb-8001").map(|e| e.code.as_str()), Some("HYB-E8001"));
    assert!(reg.lookup("HYB-HYB-3003").is_none());

    let d = Diagnostic::error("HYB-3003", "t.hy", Span::point(Pos::new(1, 1)), "msg").with_notes(vec!["".into(), "   ".into()]);
    assert!(d.notes.is_empty());
    assert!(!envelope(&[d]).contains("\"notes\""));
}
