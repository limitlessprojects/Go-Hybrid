//! `check` accepts one file and exactly the documented boolean switches.
//! A malformed invocation must not report success for only part of its input.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Inputs(PathBuf);

impl Inputs {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-check-args-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create unique inputs directory");
        fs::write(root.join("good.hy"), "fn main() {}\n").unwrap();
        fs::write(root.join("bad.hy"), "fn main( {\n").unwrap();
        Self(root)
    }

    fn check(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("check")
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("run actual hyb CLI")
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove test-owned inputs");
    }
}

fn usage_error(output: Output, expected: &str) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected usage error, stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "no partial check output");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains(expected), "missing {expected}: {stderr}");
    assert!(stderr.contains("usage: hyb check"), "{stderr}");
    assert!(
        !stderr.contains("error reading"),
        "validate before file I/O"
    );
}

#[test]
fn rejects_extra_operands_before_checking_any_file() {
    let inputs = Inputs::new();
    for args in [
        vec!["good.hy", "bad.hy"],
        vec!["bad.hy", "good.hy"],
        vec!["missing.hy", "good.hy"],
        vec!["good.hy", "good.hy"],
        vec!["--json", "good.hy", "--report-managed", "bad.hy"],
    ] {
        usage_error(inputs.check(&args), "exactly one input file");
    }
}

#[test]
fn rejects_unknown_options_on_either_side_of_file() {
    let inputs = Inputs::new();
    for args in [
        vec!["--josn", "good.hy"],
        vec!["good.hy", "--josn"],
        vec!["--deny-managd", "good.hy"],
        vec!["good.hy", "-j"],
        vec!["good.hy", "--json=true"],
        vec!["--json", "missing.hy", "--josn"],
    ] {
        usage_error(inputs.check(&args), "unknown option");
    }
}

#[test]
fn requires_a_file_even_with_supported_flags() {
    let inputs = Inputs::new();
    usage_error(inputs.check(&[]), "usage: hyb check");
    usage_error(
        inputs.check(&["--json", "--report-managed", "--deny-managed"]),
        "usage: hyb check",
    );
}

#[test]
fn valid_single_file_accepts_all_existing_flag_orders() {
    let inputs = Inputs::new();
    for args in [
        vec!["good.hy", "--json", "--report-managed", "--deny-managed"],
        vec!["--json", "--report-managed", "--deny-managed", "good.hy"],
        vec!["--deny-managed", "good.hy", "--report-managed", "--json"],
        vec![
            "--json",
            "good.hy",
            "--json",
            "--report-managed",
            "--deny-managed",
        ],
    ] {
        let output = inputs.check(&args);
        assert!(output.status.success(), "{output:?}");
        assert!(output.stderr.is_empty(), "{output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["summary"]["file"], "good.hy");
        assert_eq!(result["summary"]["verdict"], "accept");
        assert_eq!(result["summary"]["managed"]["managed_blocks"], 0);
    }
}

#[test]
fn valid_single_file_still_reports_real_parse_errors() {
    let inputs = Inputs::new();
    let output = inputs.check(&["bad.hy", "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(result["summary"]["verdict"], "reject");
    assert!(result["summary"]["errors"].as_u64().unwrap() > 0);
}

#[test]
fn valid_single_missing_file_keeps_read_error() {
    let inputs = Inputs::new();
    let output = inputs.check(&["missing.hy"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)
        .unwrap()
        .contains("error reading missing.hy"));
}

#[test]
fn end_of_options_allows_literal_dash_prefixed_paths() {
    let inputs = Inputs::new();
    for name in ["-leading.hy", "--json", "--"] {
        fs::write(inputs.0.join(name), "fn main() {}\n").unwrap();
        let output = inputs.check(&["--json", "--", name]);
        assert!(output.status.success(), "{output:?}");
        let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["summary"]["file"], name);
        assert_eq!(result["summary"]["verdict"], "accept");
    }
    let output = inputs.check(&["--", "good.hy"]);
    assert!(output.status.success(), "{output:?}");
    let output = inputs.check(&["good.hy", "--"]);
    assert!(output.status.success(), "{output:?}");
}

#[test]
fn end_of_options_still_requires_exactly_one_file() {
    let inputs = Inputs::new();
    for args in [
        vec!["--", "good.hy", "bad.hy"],
        vec!["good.hy", "--", "--json"],
        vec!["--json", "--", "missing.hy", "good.hy"],
    ] {
        usage_error(inputs.check(&args), "exactly one input file");
    }
    usage_error(inputs.check(&["--"]), "usage: hyb check");
    usage_error(inputs.check(&["--json", "--"]), "usage: hyb check");
}
