//! `--deny-managed`: the CI policy gate over the managed hatch.
//!
//! Contract (Discussion #1, ssmurfgg04-gif — "CI should be able to
//! --deny-managed or fail when new managed blocks appear"):
//!   * a program whose verdict is ACCEPT only through `managed` fails
//!     under `--deny-managed` (exit 1, named on the output);
//!   * the same program passes plain `check` (the hatch itself is legal);
//!   * a program that needs no hatch passes even WITH `--deny-managed`;
//!   * JSON mode keeps stdout pure JSON — the deny notice goes to stderr.
//!
//! Uses the pre-registered corpus programs so the gate is tested against
//! the exact shapes the checker is specified by.

use std::process::Command;

const CORPUS02: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/checker-corpus/02-shared-map-managed.hy"
);
const CORPUS15: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/checker-corpus/15-fanin-per-task-state.hy"
);

fn hyb(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(args)
        .output()
        .expect("spawn hyb");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn managed_program_passes_plain_check() {
    let (code, stdout, _stderr) = hyb(&["check", CORPUS02]);
    assert_eq!(
        code, 0,
        "corpus 02 must check clean (ACCEPT-MANAGED): {stdout}"
    );
}

#[test]
fn deny_managed_fails_the_managed_program() {
    let (code, stdout, _stderr) = hyb(&["check", CORPUS02, "--deny-managed"]);
    assert_eq!(code, 1, "--deny-managed must fail a managed program");
    assert!(
        stdout.contains("deny-managed"),
        "output must name the policy gate: {stdout}"
    );
    assert!(
        stdout.contains("1 managed block(s)"),
        "output must carry the count: {stdout}"
    );
}

#[test]
fn deny_managed_json_keeps_stdout_json() {
    let (code, stdout, stderr) = hyb(&["check", CORPUS02, "--deny-managed", "--json"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must stay valid JSON");
    // The CLI verdict is accept/reject; ACCEPT-MANAGED is the corpus-level
    // reading (managed lint = warning, never an error).
    assert_eq!(v["summary"]["verdict"], "accept");
    assert!(
        stderr.contains("deny-managed"),
        "the deny notice must go to stderr in JSON mode: {stderr}"
    );
}

#[test]
fn deny_managed_passes_a_hatch_free_program() {
    let (code, stdout, _stderr) = hyb(&["check", CORPUS15, "--deny-managed"]);
    assert_eq!(
        code, 0,
        "corpus 15 (no managed blocks) must pass even under --deny-managed: {stdout}"
    );
}

#[test]
fn require_managed_contracts_rejects_uncontracted_corpus_02() {
    let (code, stdout, _stderr) = hyb(&["check", CORPUS02, "--require-managed-contracts", "--json"]);
    assert_eq!(code, 1, "--require-managed-contracts must reject uncontracted mutex hatch");
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["summary"]["verdict"], "reject");
    assert!(
        v["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "HYB-E0005"),
        "expected HYB-E0005 in diagnostics: {stdout}"
    );
}

#[test]
fn report_managed_separates_verified_and_unverified_contracts() {
    let (code, stdout, _stderr) = hyb(&["check", CORPUS02, "--report-managed"]);
    assert_eq!(code, 0);
    assert!(
        stdout.contains("contracts: 0 verified, 1 unverified"),
        "expected verified/unverified split in --report-managed: {stdout}"
    );
}
