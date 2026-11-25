//! Issue #645: N5's first-write map allocation, run end-to-end through
//! `hyb build`.
//!
//! The accept-side corpus program carries its stdout as
//! `// EXPECTED-OUTPUT:` header lines (one per line, the corpus 35 marker);
//! the differential loop only checks that an accepted program builds and
//! exits 0, so this test diffs the output byte-exact.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CORPUS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../tests/checker-corpus");

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hyb-{}-{}", tag, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// `hyb build <src> -o <dir>/prog`, then run it.
fn build_and_run(src: &Path, dir: &Path) -> Output {
    let bin = dir.join("prog");
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("build")
        .arg(src)
        .arg("-o")
        .arg(&bin)
        .current_dir(dir)
        .output()
        .expect("run hyb build");
    assert!(
        build.status.success(),
        "hyb build of {} failed\nstdout: {}\nstderr: {}",
        src.display(),
        String::from_utf8_lossy(&build.stdout),
        String::from_utf8_lossy(&build.stderr)
    );
    Command::new(&bin).output().expect("run the built program")
}

/// The `// EXPECTED-OUTPUT: <line>` header lines, joined as stdout.
fn expected_output(src: &str) -> String {
    let mut out = String::new();
    for line in src.lines() {
        if let Some(rest) = line.strip_prefix("// EXPECTED-OUTPUT: ") {
            out.push_str(rest);
            out.push('\n');
        }
    }
    out
}

fn corpus_output_is_byte_exact(name: &str) {
    let src = Path::new(CORPUS).join(name);
    let text = fs::read_to_string(&src).expect("read corpus program");
    let want = expected_output(&text);
    assert!(!want.is_empty(), "{name} must carry EXPECTED-OUTPUT lines");

    let dir = scratch(name.trim_end_matches(".hy"));
    let run = build_and_run(&src, &dir);
    let _ = fs::remove_dir_all(&dir);
    assert!(
        run.status.success(),
        "{name} exited {:?}\nstderr: {}",
        run.status,
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(
        String::from_utf8(run.stdout).expect("UTF-8 stdout"),
        want,
        "{name}: stdout disagrees with its EXPECTED-OUTPUT header"
    );
}

#[test]
fn n5_zero_map_corpus_program_output() {
    corpus_output_is_byte_exact("n5-zero-map-first-write.hy");
}

#[test]
fn expected_output_reads_one_line_per_marker() {
    let src = "// EXPECTED-VERDICT: ACCEPT\n// EXPECTED-OUTPUT: true\n// EXPECTED-OUTPUT: x 3\nfn main() {}\n";
    assert_eq!(expected_output(src), "true\nx 3\n");
}

#[test]
fn n5_allocation_sites_corpus_program_output() {
    corpus_output_is_byte_exact("n5-zero-map-allocation-sites.hy");
}
