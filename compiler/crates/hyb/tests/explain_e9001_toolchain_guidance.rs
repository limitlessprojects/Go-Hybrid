//! HYB-E9001 is the first diagnostic a release user hits when their Go is not
//! the pinned build, and its fix-its used to be written for the maintainer's
//! machine: `source tools/env.sh so PATH picks up the pinned Go at
//! /home/z/tools/go`. A tarball install has no `tools/env.sh` and no /home/z,
//! so the guidance was unreachable exactly where it mattered.
//!
//! These tests pin the user-facing surface: `hyb explain E9001` and the real
//! `hyb build` failure both name go.dev/dl, the release install notes, and the
//! documented local escape hatch — and neither leaks a build-host path.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn unique_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "hyb-e9001-{}-{}-{tag}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    root
}

fn shim(dir: &Path, name: &str, stdout: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\necho \"{stdout}\"\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn combined(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn explain_and_build_guide_the_user_not_the_build_host() {
    let root = unique_root("explain");
    let explain = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["explain", "E9001"])
        .output()
        .expect("run hyb explain");
    assert!(explain.status.success(), "{}", combined(&explain));
    let text = combined(&explain);
    assert!(text.contains("https://go.dev/dl/"), "{text}");
    assert!(text.contains("tools/RELEASE-INSTALL.md"), "{text}");
    assert!(text.contains("HYB_ALLOW_TOOLCHAIN_DRIFT=1"), "{text}");
    assert!(!text.contains("/home/"), "build-host path leaked:\n{text}");

    // A release user with the wrong Go on PATH gets the same guidance from
    // the build itself (the toolchain gate runs before file I/O).
    shim(&root, "go", "go version go1.21.0 linux/amd64");
    fs::write(root.join("prog.hy"), "fn main() {}\n").unwrap();
    let build = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["build", "prog.hy", "-o", "app"])
        .env_remove("HYB_ALLOW_TOOLCHAIN_DRIFT")
        .env("PATH", &root)
        .current_dir(&root)
        .output()
        .expect("run hyb build");
    let build_text = combined(&build);
    assert_eq!(build.status.code(), Some(2), "{build_text}");
    assert!(build_text.contains("HYB-E9001"), "{build_text}");
    assert!(
        build_text.contains("found go1.21.0, pinned"),
        "{build_text}"
    );
    assert!(build_text.contains("https://go.dev/dl/"), "{build_text}");
    assert!(
        build_text.contains("HYB_ALLOW_TOOLCHAIN_DRIFT=1"),
        "{build_text}"
    );
    assert!(
        !build_text.contains("/home/"),
        "build-host path leaked:\n{build_text}"
    );
    fs::remove_dir_all(&root).unwrap();
}
