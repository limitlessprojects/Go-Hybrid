use hy_diag::{assert_severity_code_coherence, Diagnostic, Pos, Registry, Severity, Span};

#[test]
fn diagnostic_constructors_and_coherence_accept_all_valid_aliases_and_cases() {
    let span = Span::point(Pos::new(1, 1));
    for alias in [
        "HYB-E8001",
        "hyb-e8001",
        "E8001",
        "e8001",
        "8001",
        "managed-share",
        "MANAGED-SHARE",
    ] {
        assert_severity_code_coherence(alias, Severity::Warning);
        let d = Diagnostic::warning(alias, "test.hy", span, "managed block used");
        assert_eq!(d.code, "HYB-E8001");
        assert_eq!(d.severity, Severity::Warning);
        assert_eq!(d.name, "managed-share");
    }

    for alias in [
        "HYB-E3003",
        "hyb-e3003",
        "E3003",
        "e3003",
        "3003",
        "use-after-move",
        "Use-After-Move",
    ] {
        assert_severity_code_coherence(alias, Severity::Error);
        let d = Diagnostic::error(alias, "test.hy", span, "moved value used");
        assert_eq!(d.code, "HYB-E3003");
        assert_eq!(d.severity, Severity::Error);
        assert_eq!(d.name, "use-after-move");
    }
}

#[test]
#[cfg(debug_assertions)]
fn diagnostic_error_rejects_lint_codes_across_all_spellings() {
    let span = Span::point(Pos::new(1, 1));
    for alias in ["HYB-E8001", "hyb-e8001", "E8001", "8001", "managed-share"] {
        let res = std::panic::catch_unwind(|| {
            let _ = Diagnostic::error(alias, "test.hy", span, "lint as error");
        });
        assert!(
            res.is_err(),
            "expected Diagnostic::error({alias:?}) to panic on lint code"
        );
    }
}

#[test]
#[cfg(debug_assertions)]
fn diagnostic_warning_rejects_error_codes_across_all_spellings() {
    let span = Span::point(Pos::new(1, 1));
    for alias in ["HYB-E3001", "hyb-e3001", "E3001", "3001", "use-after-move"] {
        let res = std::panic::catch_unwind(|| {
            let _ = Diagnostic::warning(alias, "test.hy", span, "error as warning");
        });
        assert!(
            res.is_err(),
            "expected Diagnostic::warning({alias:?}) to panic on non-lint code"
        );
    }
}

#[test]
fn registry_lookup_accepts_case_insensitive_names_and_rejects_malformed_prefixes() {
    let reg = Registry::global();
    assert_eq!(
        reg.lookup("USE-AFTER-MOVE").map(|e| e.code.as_str()),
        Some("HYB-E3003")
    );
    assert_eq!(
        reg.lookup("  Use-After-Move ").map(|e| e.code.as_str()),
        Some("HYB-E3003")
    );
    assert_eq!(
        reg.lookup("Managed-Share").map(|e| e.code.as_str()),
        Some("HYB-E8001")
    );

    for malformed in [
        "",
        "   ",
        "HYB-HYB-E3003",
        "HYB-HYB-3003",
        "HYB-3003",
        "EEE3003",
        "HYB-EE3003",
    ] {
        assert!(
            reg.lookup(malformed).is_none(),
            "expected malformed query {malformed:?} to return None"
        );
    }
}

#[test]
fn span_to_merges_forward_reversed_and_nested_spans_symmetrically() {
    let early = Span::new(Pos::new(2, 4), Pos::new(2, 10));
    let late = Span::new(Pos::new(5, 1), Pos::new(5, 8));
    let expected = Span::new(Pos::new(2, 4), Pos::new(5, 8));
    assert_eq!(early.to(late), expected);
    assert_eq!(late.to(early), expected);

    let outer = Span::new(Pos::new(1, 1), Pos::new(10, 20));
    let inner = Span::new(Pos::new(3, 5), Pos::new(4, 12));
    assert_eq!(outer.to(inner), outer);
    assert_eq!(inner.to(outer), outer);
}
