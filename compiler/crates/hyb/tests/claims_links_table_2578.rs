//! Guard test for Issue #2578: GUARANTEES.md claims table must contain
//! numbered claim rows so check_claims_links.py never sees an empty table.

const GUARANTEES_MD: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../GUARANTEES.md"
);

#[test]
fn guarantees_md_contains_claims_table_section_and_rows() {
    let text = std::fs::read_to_string(GUARANTEES_MD).expect("read GUARANTEES.md");
    assert!(
        text.contains("## The claims table"),
        "GUARANTEES.md must contain '## The claims table' heading"
    );
    let row_count = text
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            trimmed.starts_with('|')
                && trimmed
                    .split('|')
                    .nth(1)
                    .map(|cell| !cell.trim().is_empty() && cell.trim().chars().all(|c| c.is_ascii_digit()))
                    .unwrap_or(false)
        })
        .count();
    assert!(
        row_count >= 19,
        "expected at least 19 numbered claim rows in GUARANTEES.md, found {row_count}"
    );
}
