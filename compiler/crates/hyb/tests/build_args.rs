//! #1617 — `hyb build` validated its invocation with ad-hoc `.position()`
//! lookups: unknown options, missing values and unpaired package-mode flags
//! silently fell back to defaults, and `--deny-managed` was ignored. Every
//! malformed invocation must exit 2 before touching the filesystem, and the
//! policy gate must fail a managed program before emission.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Inputs(PathBuf);

impl Inputs {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-build-args-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("create unique inputs directory");
        fs::write(root.join("good.hy"), "fn main() {}\n").unwrap();
        fs::write(
            root.join("managed.hy"),
            "fn main() {\n    managed(\"why\", \"mutex\") {\n        _ = 1\n    }\n}\n",
        )
        .unwrap();
        Self(root)
    }

    fn build(&self, args: &[&str]) -> Output {
        self.build_with_drift(args, false)
    }

    fn build_with_drift(&self, args: &[&str], allow_drift: bool) -> Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_hyb"));
        command.arg("build").args(args).current_dir(&self.0);
        if allow_drift {
            command.env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1");
        }
        command.output().expect("run actual hyb CLI")
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).expect("remove test-owned inputs");
    }
}

fn usage_error(output: Output) {
    assert_eq!(
        output.status.code(),
        Some(2),
        "expected usage error, stdout={}, stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty(), "no output before validation");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("usage: hyb build"), "{stderr}");
    assert!(
        !stderr.contains("error reading"),
        "validate before file I/O: {stderr}"
    );
    assert!(
        !stderr.contains("error [emit]"),
        "validate before emission: {stderr}"
    );
}

#[test]
fn rejects_unknown_and_malformed_options() {
    let inputs = Inputs::new();
    for args in [
        vec!["good.hy", "--unknown-flag"],
        vec!["good.hy", "-o"],
        vec!["good.hy", "--package", "p"],
        vec!["good.hy", "--out-dir", "d"],
        vec!["good.hy", "--binding-prefix"],
        vec!["good.hy", "-o", "--emit-go"],
        vec!["good.hy", "second.hy"],
        vec!["other.txt"],
    ] {
        usage_error(inputs.build(&args));
    }
}

#[test]
fn missing_value_error_is_specific() {
    let inputs = Inputs::new();
    let output = inputs.build(&["good.hy", "-o"]);
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("-o requires a value"), "{stderr}");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn deny_managed_fails_a_managed_program_before_building() {
    let inputs = Inputs::new();
    let plain = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("check")
        .arg("managed.hy")
        .current_dir(&inputs.0)
        .output()
        .expect("run hyb check");
    assert_eq!(
        plain.status.code(),
        Some(0),
        "the managed program itself checks clean: {}",
        String::from_utf8_lossy(&plain.stdout)
    );

    let output = inputs.build_with_drift(&["managed.hy", "-o", "bin", "--deny-managed"], true);
    assert_eq!(output.status.code(), Some(1), "policy gate exits 1");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("deny-managed: 1 managed block(s)"), "{stdout}");
    assert!(!inputs.0.join("bin").exists(), "no binary on a denied build");
    assert!(!inputs.0.join("bin.go").exists(), "no Go emitted on a denied build");
}
