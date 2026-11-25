//! `hyb build --package` CLI guards (issue #4439): the package name must be a
//! valid Go identifier before any filesystem I/O, and a hand-written
//! `<out-dir>/<package>.go` must never be overwritten.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Inputs(PathBuf);

impl Inputs {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "hyb-pkg-guard-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join("good.hy"), "fn main() {}\n").unwrap();
        Self(p)
    }
}

impl Drop for Inputs {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run_pkg(dir: &Path, pkg: &str, out: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args([
            "build",
            "good.hy",
            "--package",
            pkg,
            "--out-dir",
            out.to_str().unwrap(),
        ])
        .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
        .current_dir(dir)
        .output()
        .expect("run hyb")
}

#[test]
fn invalid_package_names_are_rejected_before_fs_io() {
    let inp = Inputs::new();
    let out = inp.0.join("out");
    for name in ["../victim", "sub/pkg", "bad-name", "123pkg", "_", "default", "func"] {
        let res = run_pkg(&inp.0, name, &out);
        assert_eq!(
            res.status.code(),
            Some(2),
            "package {name:?} should be rejected, stderr: {}",
            String::from_utf8_lossy(&res.stderr)
        );
        assert!(
            String::from_utf8_lossy(&res.stderr).contains("invalid Go package name"),
            "package {name:?} should name the validation failure, stderr: {}",
            String::from_utf8_lossy(&res.stderr)
        );
        assert!(
            !out.exists(),
            "package {name:?} must be rejected before creating --out-dir"
        );
        assert!(
            !inp.0.join("victim.go").exists(),
            "package {name:?} must not escape --out-dir"
        );
    }
}

#[test]
fn hand_written_package_output_is_preserved() {
    let inp = Inputs::new();
    let out = inp.0.join("out");
    fs::create_dir_all(&out).unwrap();
    let pkg_go = out.join("mypkg.go");
    let hand = b"package mypkg\n// hand-written: must survive\n";
    fs::write(&pkg_go, hand).unwrap();

    let res = run_pkg(&inp.0, "mypkg", &out);
    assert_eq!(
        res.status.code(),
        Some(2),
        "a hand-written package file must be refused, stderr: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    assert!(
        String::from_utf8_lossy(&res.stderr).contains("refusing to overwrite"),
        "stderr should explain the refusal, got: {}",
        String::from_utf8_lossy(&res.stderr)
    );
    assert_eq!(fs::read(&pkg_go).unwrap(), hand);
}

#[test]
fn non_utf8_package_output_is_preserved() {
    let inp = Inputs::new();
    let out = inp.0.join("out");
    fs::create_dir_all(&out).unwrap();
    let pkg_go = out.join("mypkg.go");
    let non_utf8 = [47, 47, 32, 67, 111, 100, 101, 255, 254, 0, 10];
    fs::write(&pkg_go, non_utf8).unwrap();

    let res = run_pkg(&inp.0, "mypkg", &out);
    assert_eq!(res.status.code(), Some(2));
    assert_eq!(fs::read(&pkg_go).unwrap(), non_utf8);
}
