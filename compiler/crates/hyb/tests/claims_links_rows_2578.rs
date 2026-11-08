//! Guard test for Issue #2578: tools/check_claims_links.py must fail closed
//! when zero claims rows are parsed from GUARANTEES.md.

const CHECK_CLAIMS_LINKS_PY: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tools/check_claims_links.py"
);

#[test]
fn check_claims_links_script_enforces_non_empty_rows_guard() {
    let script = std::fs::read_to_string(CHECK_CLAIMS_LINKS_PY)
        .expect("read tools/check_claims_links.py");
    assert!(
        script.contains("if not rows:"),
        "check_claims_links.py must fail closed when not rows"
    );
    assert!(
        script.contains("\"no-claims-rows\""),
        "check_claims_links.py must report no-claims-rows when zero rows are parsed"
    );
}
