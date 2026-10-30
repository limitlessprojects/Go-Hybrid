//! Integration tests for `hyb build` CLI option validation and `--deny-managed`
//! enforcement (issue #1319).

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

const CORPUS02_MANAGED: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../tests/checker-corpus/02-shared-map-managed.hy"
);

struct Workdir(PathBuf);

impl Workdir {
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!(
            "hyb-build-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        fs::write(dir.join("clean.hy"), "fn main() {}\n").unwrap();
        Self(dir)
    }

    fn build(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .arg("build")
            .args(args)
            .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
            .current_dir(&self.0)
            .output()
            .expect("spawn hyb build")
    }
}

impl Drop for Workdir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn build_deny_managed_rejects_managed_program_and_passes_clean_program() {
    let w = Workdir::new();
    let bad = w.build(&["--deny-managed", "--emit-go", "-o", "Denied", CORPUS02_MANAGED]);
    assert_eq!(bad.status.code(), Some(1), "expected exit 1: {bad:?}");
    let stderr = String::from_utf8_lossy(&bad.stderr);
    assert!(
        stderr.contains("deny-managed"),
        "stderr must mention deny-managed: {stderr}"
    );
    assert!(
        !w.0.join("Denied.go").exists(),
        "must not emit Go when --deny-managed fails"
    );

    let ok = w.build(&["--deny-managed", "--emit-go", "-o", "Clean", "clean.hy"]);
    assert_eq!(ok.status.code(), Some(0), "clean build must pass: {ok:?}");
    assert!(w.0.join("Clean.go").exists(), "expected emitted Go file");
}

#[test]
fn build_rejects_unknown_flags_missing_values_and_half_configured_package_mode() {
    let w = Workdir::new();
    for (args, expected) in [
        (vec!["--unknown-flag", "clean.hy"], "unknown build option"),
        (vec!["clean.hy", "-o"], "-o requires a value"),
        (
            vec!["clean.hy", "-o", "--emit-go"],
            "-o requires a value, not an option",
        ),
        (
            vec!["clean.hy", "-o", "a", "-o", "b"],
            "-o may only be supplied once",
        ),
        (
            vec!["clean.hy", "--package", "foo"],
            "--package and --out-dir must be supplied together",
        ),
        (
            vec!["clean.hy", "--out-dir", "pkgout"],
            "--package and --out-dir must be supplied together",
        ),
        (
            vec!["clean.hy", "clean.hy"],
            "build expects exactly one .hy input file",
        ),
    ] {
        let out = w.build(&args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "expected exit 2 for {args:?}, got {out:?}"
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expected),
            "expected `{expected}` in stderr for {args:?}, got: {stderr}"
        );
    }
}
