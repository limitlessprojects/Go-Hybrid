//! Integration test: EVERY must-parse file parses cleanly.
//!

//!   * tests/checker-corpus/*.hy            (129 programs — the checker corpus;
//!     61-string-escape-fail-closed.hy is the corpus's first PARSE-reject
//!     fixture and is pinned to fail closed instead)
//!   * spikes/spike6-ownership/work/*.hy    (10 programs — Tier-0 rewrites)
//!   * examples/01,02,03/program.hy         (MVP-0 target programs)
//!
//! plus the stretch examples 04–10 (same surface as the spikes; must parse
//! or fail cleanly — here we assert they parse too).

use hy_syntax::parse_file;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn hy_files_under(rel: &str) -> Vec<PathBuf> {
    let dir = repo_root().join(rel);
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", dir.display(), e))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "hy").unwrap_or(false))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no .hy files under {}", dir.display());
    files
}

fn must_parse(path: &PathBuf) {
    let src = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    match parse_file(&path.display().to_string(), &src) {
        Ok(_) => {}
        Err(diags) => {
            let rendered: Vec<String> = diags.iter().map(|d| d.render_head()).collect();
            panic!(
                "PARSE FAILED: {}\n{} diagnostic(s):\n{}",
                path.display(),
                diags.len(),
                rendered.join("\n")
            );
        }
    }
}

#[test]
fn checker_corpus_all_parse() {
    let files = hy_files_under("tests/checker-corpus");

    assert_eq!(files.len(), 129, "expected exactly 129 corpus programs",);
    // 61-string-escape-fail-closed.hy is the corpus's first PARSE-reject
    // fixture (a malformed LX7 string escape must be named by the lexer,
    // never silently accepted): it is excluded from the must-parse loop
    // and pinned to fail closed by corpus_parse_reject_fixtures_fail_closed.
    let parse_rejects = ["61-string-escape-fail-closed.hy"];
    for f in &files {
        let name = f.file_name().unwrap().to_str().unwrap();
        if !parse_rejects.contains(&name) {
            must_parse(f);
        }
    }
}

#[test]
fn corpus_parse_reject_fixtures_fail_closed() {
    // The lexer's parse-reject corpus fixture: `"\u{41}"` (the LX6 rune
    // form inside an interpreted string) must FAIL to parse with
    // HYB-E0002 tagged rule LX7 — before the fix it was silently
    // accepted and the value corrupted to "41}".
    let path = repo_root()
        .join("tests/checker-corpus")
        .join("61-string-escape-fail-closed.hy");
    let src = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", path.display(), e));
    match parse_file(&path.display().to_string(), &src) {
        Ok(_) => panic!(
            "61-string-escape-fail-closed.hy must not parse cleanly: the \\u{{…}} rune form in a string is a malformed LX7 escape"
        ),
        Err(diags) => {
            assert!(
                diags
                    .iter()
                    .any(|d| d.code == "HYB-E0002" && d.tier0_rule.as_deref() == Some("LX7")),
                "expected HYB-E0002 tagged LX7, got: {:?}",
                diags.iter().map(|d| d.render_head()).collect::<Vec<_>>()
            );
        }
    }
}

#[test]
fn spike6_ownership_work_all_10_parse() {
    let files = hy_files_under("spikes/spike6-ownership/work");
    assert_eq!(files.len(), 10, "expected exactly 10 spike-6 programs");
    for f in &files {
        must_parse(f);
    }
}

#[test]
fn examples_01_02_03_parse() {
    for rel in ["01-hello", "02-cli-cat", "03-worker-pool"] {
        let path = repo_root().join("examples").join(rel).join("program.hy");
        must_parse(&path);
    }
}

#[test]
fn stretch_examples_04_to_10_parse() {
    for rel in [
        "04-pipeline-backpressure",
        "05-json-transform",
        "06-http-server",
        "07-lru-generics",
        "08-graph-ownership",
        "09-go-imports-hybrid",
        "10-rust-crate-import",
    ] {
        let path = repo_root().join("examples").join(rel).join("program.hy");
        if path.exists() {
            must_parse(&path);
        }
    }
}
#[test]
fn fn_type_result_does_not_swallow_next_line_727() {
    parse_file("t.hy", "fn main() {\n    total := 0\n    var cb fn() int\n    total = 7\n}").unwrap();
}
