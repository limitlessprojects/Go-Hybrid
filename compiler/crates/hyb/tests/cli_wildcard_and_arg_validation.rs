//! Integration tests for `hyb` CLI path and argument validation:
//! 1. `hyb generate` supports Go recursive package wildcards (`./...`, `...`,
//!    `<dir>/...`) advertised for `//go:generate hyb generate ./...`, and
//!    deduplicates overlapping `.hy` inputs (`./calc.hy` vs `calc.hy` vs `.`).
//! 2. `hyb build` rejects duplicate `-o`, `--package`, `--out-dir`, and
//!    `--binding-prefix` flags and rejects conflicting binary-mode (`-o`,
//!    `--emit-go`) vs package-mode (`--package`, `--out-dir`, `--binding-prefix`)
//!    flag combinations before touching the filesystem.
//! 3. `hyb explain` and `hyb vendor-runtime` reject unknown options and extra
//!    positional operands with exit code 2 before performing any work.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new(tag: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "hyb-cli-wildcard-{}-{}-{}",
            tag,
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("subpkg")).expect("create fixture dirs");
        fs::write(
            root.join("go.mod"),
            "module example.com/hybrid-demo\n\ngo 1.26\n",
        )
        .unwrap();
        fs::write(root.join("app.go"), "package demo\n").unwrap();
        fs::write(
            root.join("calc.hy"),
            "fn add(a int, b int) int {\n    return a + b\n}\n",
        )
        .unwrap();
        fs::write(root.join("subpkg/sub.go"), "package subpkg\n").unwrap();
        fs::write(
            root.join("subpkg/math.hy"),
            "fn mul(a int, b int) int {\n    return a * b\n}\n",
        )
        .unwrap();
        Self(root)
    }

    fn hyb(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_hyb"))
            .args(args)
            .current_dir(&self.0)
            .output()
            .expect("spawn hyb")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generate_supports_go_ellipsis_wildcards_and_deduplicates_overlapping_inputs() {
    let f = Fixture::new("ellipsis");

    let out = f.hyb(&["generate", "./..."]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("generate: 2 file(s) written, 2 total"),
        "expected 2 files written from ./..., got: {stdout}"
    );
    assert!(f.0.join("calc.hy.go").is_file());
    assert!(f.0.join("subpkg/math.hy.go").is_file());

    for check_arg in ["./...", "...", "subpkg/..."] {
        let check = f.hyb(&["generate", "--check", check_arg]);
        assert_eq!(
            check.status.code(),
            Some(0),
            "check {check_arg} failed: stdout={} stderr={}",
            String::from_utf8_lossy(&check.stdout),
            String::from_utf8_lossy(&check.stderr)
        );
    }

    // Overlapping paths (`./calc.hy`, `calc.hy`, `.`) must deduplicate rather
    // than processing `calc.hy` multiple times.
    let dedup = f.hyb(&["generate", "--check", "./calc.hy", "calc.hy"]);
    assert_eq!(dedup.status.code(), Some(0));
    assert!(
        String::from_utf8_lossy(&dedup.stdout).contains("generate --check: 1 file(s) fresh"),
        "expected deduplicated count of 1, got: {}",
        String::from_utf8_lossy(&dedup.stdout)
    );
}

#[test]
fn build_rejects_duplicate_and_conflicting_mode_options_before_io() {
    let f = Fixture::new("build-modes");
    for (args, expected) in [
        (
            vec!["build", "calc.hy", "-o", "one", "-o", "two"],
            "-o may only be supplied once",
        ),
        (
            vec![
                "build",
                "calc.hy",
                "--package",
                "p1",
                "--package",
                "p2",
                "--out-dir",
                "d",
            ],
            "--package may only be supplied once",
        ),
        (
            vec![
                "build",
                "calc.hy",
                "--package",
                "p",
                "--out-dir",
                "d1",
                "--out-dir",
                "d2",
            ],
            "--out-dir may only be supplied once",
        ),
        (
            vec![
                "build",
                "calc.hy",
                "--package",
                "p",
                "--out-dir",
                "d",
                "--binding-prefix",
                "a",
                "--binding-prefix",
                "b",
            ],
            "--binding-prefix may only be supplied once",
        ),
        (
            vec![
                "build",
                "calc.hy",
                "--package",
                "p",
                "--out-dir",
                "d",
                "-o",
                "bin",
            ],
            "-o and --emit-go cannot be combined with --package/--out-dir",
        ),
        (
            vec![
                "build",
                "calc.hy",
                "--package",
                "p",
                "--out-dir",
                "d",
                "--emit-go",
            ],
            "-o and --emit-go cannot be combined with --package/--out-dir",
        ),
        (
            vec!["build", "calc.hy", "--binding-prefix", "custom/rt"],
            "--binding-prefix requires --package and --out-dir",
        ),
    ] {
        let out = f.hyb(&args);
        assert_eq!(out.status.code(), Some(2), "args={args:?}");
        assert!(out.stdout.is_empty(), "args={args:?}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            stderr.contains(expected),
            "expected `{expected}` in `{stderr}`"
        );
    }
}

#[test]
fn explain_and_vendor_runtime_reject_unknown_options_and_extra_operands() {
    let f = Fixture::new("explain-vendor");

    for args in [
        vec!["explain", "--json", "HYB-E0101"],
        vec!["explain", "-h"],
        vec!["explain", "HYB-E0101", "HYB-E0102"],
        vec!["explain", "--"],
    ] {
        let out = f.hyb(&args);
        assert_eq!(out.status.code(), Some(2), "args={args:?}");
        assert!(out.stdout.is_empty(), "args={args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: hyb explain"),
            "args={args:?} stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let ok = f.hyb(&["explain", "--", "HYB-E0101"]);
    assert_eq!(ok.status.code(), Some(0));

    for args in [
        vec!["vendor-runtime", "-x"],
        vec!["vendor-runtime", "--unknown", "target"],
        vec!["vendor-runtime", "dir1", "dir2"],
        vec!["vendor-runtime", "--"],
    ] {
        let out = f.hyb(&args);
        assert_eq!(out.status.code(), Some(2), "args={args:?}");
        assert!(out.stdout.is_empty(), "args={args:?}");
        assert!(
            !f.0.join("-x").exists(),
            "single-dash flag must not create a directory named `-x`"
        );
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("usage: hyb vendor-runtime"),
            "args={args:?} stderr={}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}
