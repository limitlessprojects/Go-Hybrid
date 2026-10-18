//! `--report-cancel` / `--deny-cancel` (W-006): the lint never changes the
//! default verdict, `--report-cancel` exits 0 on warnings, `--deny-cancel`
//! fails the check by policy (and implies `--report-cancel`).

use std::process::Command;

const CLEAN: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../spikes/w006-cancel-safety/cases/p1-select-token-arm.hy"
);
const OBLIVIOUS: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../spikes/w006-cancel-safety/cases/n1-bare-recv-ignores-done.hy"
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
fn default_check_ignores_the_lint() {
    let (code, stdout, _) = hyb(&["check", OBLIVIOUS]);
    assert_eq!(code, 0, "{stdout}");
    assert!(!stdout.contains("HYB-E8002"), "{stdout}");
}

#[test]
fn report_cancel_warns_but_passes() {
    let (code, stdout, _) = hyb(&["check", OBLIVIOUS, "--report-cancel"]);
    assert_eq!(code, 0, "{stdout}");
    assert!(stdout.contains("warning[HYB-E8002]"), "{stdout}");
    assert!(
        stdout.contains("cancel: 1 cancel-oblivious site(s)"),
        "{stdout}"
    );
}

#[test]
fn deny_cancel_fails_oblivious_program() {
    let (code, stdout, _) = hyb(&["check", OBLIVIOUS, "--deny-cancel"]);
    assert_eq!(code, 1, "{stdout}");
    assert!(
        stdout.contains("HYB-E8002"),
        "implies --report-cancel: {stdout}"
    );
    assert!(stdout.contains("deny-cancel: 1"), "{stdout}");
}

#[test]
fn deny_cancel_passes_clean_program() {
    let (code, stdout, _) = hyb(&["check", CLEAN, "--deny-cancel"]);
    assert_eq!(code, 0, "{stdout}");
}

#[test]
fn deny_cancel_json_keeps_stdout_pure() {
    let (code, stdout, stderr) = hyb(&["check", OBLIVIOUS, "--deny-cancel", "--json"]);
    assert_eq!(code, 1);
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("stdout must stay valid JSON");
    assert_eq!(v["diagnostics"][0]["code"], "HYB-E8002");
    assert!(stderr.contains("deny-cancel"), "{stderr}");
}
