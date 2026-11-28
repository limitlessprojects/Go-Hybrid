//! The Rust pin (HYB-E9001) governs only what rustc produces: rebuilding the
//! hyrt staticlib. A bridge-free `hyb build` never runs rustc, so a different
//! rustc on PATH (any Rust developer's stable) must not block it — release
//! installs promise "No Rust toolchain needed" (tools/RELEASE-INSTALL.md).
//!
//! Hermetic: PATH holds only shim `go` (reports the pinned version) and shim
//! `rustc` (reports a version that is not the pin), so the machine's real
//! toolchains are irrelevant.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Command, Output};

const PINNED_GO: &str = match option_env!("HYB_PINNED_GO") {
    Some(v) => v,
    None => "go1.26.9",
};

fn shim(dir: &Path, name: &str, stdout: &str) {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\necho \"{stdout}\"\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn hyb(args: &[&str], rustc_version_line: &str) -> Output {
    let root =
        std::env::temp_dir().join(format!("hyb-rust-pin-{}-{}", std::process::id(), args[0]));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    shim(&root, "go", &format!("go version {PINNED_GO} linux/amd64"));
    shim(&root, "rustc", rustc_version_line);
    fs::write(root.join("good.hy"), "fn main() {}\n").unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(args)
        .env_remove("HYB_ALLOW_TOOLCHAIN_DRIFT")
        .env("PATH", &root)
        .current_dir(&root)
        .output()
        .expect("run hyb");
    fs::remove_dir_all(&root).unwrap();
    out
}

#[test]
fn bridge_free_build_ignores_a_non_pinned_rustc() {
    let out = hyb(
        &["build", "good.hy", "-o", "app", "--emit-go"],
        "rustc 1.100.0 (0ef1bc386 2026-11-09)",
    );
    assert!(
        out.status.success(),
        "a bridge-free build must not depend on rustc: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn rust_pin_still_gates_where_rustc_is_used() {
    // bench-overhead rebuilds/measures the bridge, so the pinned rustc matters.
    let out = hyb(&["bench-overhead"], "rustc 1.100.0 (0ef1bc386 2026-11-09)");
    let all = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(!out.status.success(), "{all}");
    assert!(
        all.contains("HYB-E9001") && all.contains("found rustc 1.100.0"),
        "{all}"
    );
}
