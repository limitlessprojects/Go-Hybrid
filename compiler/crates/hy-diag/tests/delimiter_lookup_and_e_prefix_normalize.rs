use hy_diag::{assert_severity_code_coherence, Diagnostic, Pos, Registry, Severity, Span};

#[test]
fn parenthesized_and_angle_delimiters_lookup() {
    let reg = Registry::global();
    for q in [
        "(HYB-E3003)",
        "<HYB-E3003>",
        "(E3003)",
        "<E3003>",
        "(3003)",
        "<3003>",
        "HYB-E3003,",
        "HYB-E3003.",
        "HYB-E3003;",
        "(use-after-move)",
        "<use-after-move>",
    ] {
        assert_eq!(
            reg.lookup(q).map(|e| e.code.as_str()),
            Some("HYB-E3003"),
            "failed lookup for query: {q}"
        );
    }
}

#[test]
fn hyphen_and_underscore_e_prefix_lookup() {
    let reg = Registry::global();
    for q in [
        "E-3003",
        "e-3003",
        "E_3003",
        "e_3003",
        "HYB-E-3003",
        "hyb-e-3003",
        "HYB_E_3003",
        "hyb_e_3003",
        "HYB-use-after-move",
        "hyb-use-after-move",
    ] {
        assert_eq!(
            reg.lookup(q).map(|e| e.code.as_str()),
            Some("HYB-E3003"),
            "failed lookup for query: {q}"
        );
    }
}

#[test]
fn parenthesized_lint_code_coherence_assert() {
    assert_severity_code_coherence("(HYB-E8001)", Severity::Warning);
    assert_severity_code_coherence("<HYB-E8001>", Severity::Warning);
    assert_severity_code_coherence("(E8001)", Severity::Warning);
    assert_severity_code_coherence("<E8001>", Severity::Warning);
    assert_severity_code_coherence("(8001)", Severity::Warning);
    assert_severity_code_coherence("E-8001", Severity::Warning);
    assert_severity_code_coherence("e_8001", Severity::Warning);
    assert_severity_code_coherence("HYB-E-8001", Severity::Warning);

    let w = Diagnostic::warning("(E8001)", "t.hy", Span::point(Pos::new(1, 1)), "lint");
    assert_eq!(w.code, "HYB-E8001");
}

#[test]
fn prefixed_machine_names_are_case_insensitive() {
    let reg = Registry::global();
    for q in [
        "HyB-use-after-move",
        "hYb_UsE_aFtEr_MoVe",
        "(HyB-use-after-move),",
    ] {
        assert_eq!(
            reg.lookup(q).map(|e| e.code.as_str()),
            Some("HYB-E3003"),
            "{q}"
        );
    }
}

#[test]
fn normalization_keeps_unknown_and_internal_punctuation_invalid() {
    let reg = Registry::global();
    for q in [
        "",
        "()",
        "<>",
        "E-30,03",
        "HYB-E424242",
        "HYB-not-a-real-diagnostic",
    ] {
        assert!(reg.lookup(q).is_none(), "{q}");
    }
}
