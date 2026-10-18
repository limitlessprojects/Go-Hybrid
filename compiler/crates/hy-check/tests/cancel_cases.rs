//! W-006: each case's HYB-E8002 count must match its `EXPECTED-CANCEL` header.

use hy_check::check_cancel;
use hy_syntax::parse_file;
use std::path::Path;

#[test]
fn cancel_cases_match_preregistered_counts() {
    let dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../spikes/w006-cancel-safety/cases");
    let mut seen = 0;
    let mut bad = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("cases dir") {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("hy") {
            continue;
        }
        let src = std::fs::read_to_string(&path).unwrap();
        let header = src
            .lines()
            .find_map(|l| l.strip_prefix("// EXPECTED-CANCEL: "))
            .unwrap_or_else(|| panic!("{}: no EXPECTED-CANCEL header", path.display()));
        let want: usize = match header.trim() {
            "CLEAN" => 0,
            h => h.rsplit('x').next().unwrap().parse().expect("E8002 xN"),
        };
        let name = path.display().to_string();
        let ast = parse_file(&name, &src).unwrap_or_else(|d| panic!("{}: parse: {:?}", name, d));
        let got = check_cancel(&ast).diagnostics.len();
        if got != want {
            bad.push(format!("{}: want {} got {}", name, want, got));
        }
        seen += 1;
    }
    assert!(
        seen >= 30,
        "expected the 30 registered cases, found {}",
        seen
    );
    assert!(bad.is_empty(), "mismatches:\n{}", bad.join("\n"));
}
