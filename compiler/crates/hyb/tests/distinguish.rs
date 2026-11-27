//! End-to-end `hyb distinguish` coverage for #4807. These tests exercise the
//! real parser/checker, AST-backed finite-state extraction, product search,
//! exact cover, and stable JSON pack emission through the built CLI.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::Value;

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../spikes/w4807-recognizer-probe-pack/hy")
}

fn run(args: &[String]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("distinguish")
        .args(args)
        .output()
        .expect("run hyb distinguish")
}

fn grammar_args() -> Vec<String> {
    let root = fixture_root();
    (0..8)
        .map(|code| {
            let stem = format!("grammar-{code:03b}");
            format!("{stem}={}", root.join(format!("{stem}.hy")).display())
        })
        .collect()
}

fn tag_args() -> Vec<String> {
    let root = fixture_root();
    ["base", "base-extra-state", "a", "b", "c", "d"]
        .into_iter()
        .map(|name| format!("{name}={}", root.join(format!("tag-{name}.hy")).display()))
        .collect()
}

#[test]
fn extracts_eight_hy_revisions_and_emits_three_probe_minimum() {
    let first = run(&grammar_args());
    assert_eq!(
        first.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let pack: Value = serde_json::from_slice(&first.stdout).expect("valid pack JSON");
    assert_eq!(pack["schema"], "hyb.recognizer-probe-pack/v1");
    assert_eq!(pack["certificate"]["status"], "minimal");
    assert_eq!(pack["certificate"]["product_exhaustive"], true);
    assert_eq!(pack["certificate"]["cover_exhaustive"], true);
    assert_eq!(pack["certificate"]["product_states"], 8);
    assert_eq!(pack["certificate"]["probe_count"], 3);
    assert_eq!(pack["certificate"]["total_probe_bytes"], 4);
    let mut probes: Vec<&str> = pack["probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|probe| probe["input_base64"].as_str().unwrap())
        .collect();
    probes.sort_unstable();
    assert_eq!(probes, vec!["KzA=", "MA==", "Xw=="], "+0, 0, and _");

    let second = run(&grammar_args());
    assert_eq!(second.status.code(), Some(0));
    assert_eq!(
        first.stdout, second.stdout,
        "pack ordering is deterministic"
    );
}

#[test]
fn four_tags_and_equivalent_extra_state_revision_need_four_probes() {
    let output = run(&tag_args());
    assert_eq!(
        output.status.code(),
        Some(0),
        "stdout={}\nstderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let pack: Value = serde_json::from_slice(&output.stdout).expect("valid pack JSON");
    assert_eq!(pack["certificate"]["status"], "minimal");
    assert_eq!(pack["certificate"]["probe_count"], 4);
    assert_eq!(pack["equivalence_proven"], true);
    assert_eq!(
        pack["equivalence_classes"],
        serde_json::json!([["base", "base-extra-state"], ["a"], ["b"], ["c"], ["d"]])
    );
    let mut probes: Vec<&str> = pack["probes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|probe| probe["input_base64"].as_str().unwrap())
        .collect();
    probes.sort_unstable();
    assert_eq!(probes, vec!["QQ==", "Qg==", "Qw==", "RA=="]);
}

#[test]
fn malformed_invocations_fail_before_analysis() {
    let output = run(&["twice=x.hy".into(), "twice=y.hy".into()]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("duplicate candidate name"));
}
