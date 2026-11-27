//! #4517: the rust pin must not gate a build that never uses Rust.
//!
//! `hyb build` ran `rustc --version` up front and hard-failed (HYB-E9001) on any
//! mismatch — including a bridge-free program that never touches the bridge and
//! `tools/RELEASE-INSTALL.md`'s "No Rust toolchain needed" path. The pin governs
//! exactly one step (rebuilding the hyrt staticlib in `ensure_staticlib`), so it
//! is enforced there now.
//!
//! These tests pin both halves of that contract at the CLI:
//!
//! * a *bridge-free* build succeeds with a rustc that is NOT the pin, and says
//!   nothing about rust at all;
//! * a *bridge* build that must rebuild the staticlib still refuses with
//!   HYB-E9001 naming the rust pin (the guard moved; it did not disappear).
//!
//! Both use a PATH shim so they are host-independent: the shim `go` reports the
//! pinned go version on `go version` and forwards everything else to the real
//! go, and the shim `rustc` reports an off-pin version. That isolates the pin
//! decision from whatever toolchains the machine happens to have, which is what
//! makes the test deterministic on a dev box (go 1.27, rustc 1.97) and in CI
//! (pinned go 1.26.9, rustc 1.99.0) alike.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The go version the shim reports. Kept in sync with `PINNED_GO`'s default
/// (compiler/crates/hyb/src/main.rs); a drift here fails loudly rather than
/// silently skipping the pin.
const SHIM_GO_VERSION: &str = "go1.26.9";
/// A rustc version that is deliberately NOT the pin.
const SHIM_RUST_VERSION: &str = "1.98.0";

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "hyb-pins-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn write_shim(dir: &Path, name: &str, body: &str) {
    let path = dir.join(name);
    fs::write(&path, body).expect("write shim");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).expect("chmod shim");
}

/// Find a command on PATH, skipping our own shim directory.
fn real_bin(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for entry in std::env::split_paths(&path) {
        let candidate = entry.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

/// A shim directory: `go` reports the pinned version and delegates the rest,
/// `rustc` reports an off-pin version. `None` for `rustc` means "no rustc on
/// PATH at all" (the release-matrix arm64 shape).
fn shims(with_rustc: bool) -> PathBuf {
    let dir = scratch("shim");
    let go = real_bin("go").expect("the tests need a real go on PATH");
    write_shim(
        &dir,
        "go",
        &format!(
            "#!/bin/sh\n\
             if [ \"$1\" = \"version\" ]; then echo \"go version {SHIM_GO_VERSION} test/arch\"; exit 0; fi\n\
             exec \"{}\" \"$@\"\n",
            go.display()
        ),
    );
    if with_rustc {
        write_shim(
            &dir,
            "rustc",
            &format!(
                "#!/bin/sh\n\
                 if [ \"$1\" = \"--version\" ]; then echo \"rustc {SHIM_RUST_VERSION} (deadbeef 2025-09-28)\"; exit 0; fi\n\
                 exit 0\n"
            ),
        );
    }
    dir
}

fn build_with_path(shim_dir: &Path, program: &str, cwd: &Path, out: &str) -> Output {
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut joined = shim_dir.as_os_str().to_owned();
    joined.push(":");
    joined.push(&path);
    Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["build", program, "-o", out])
        .env("PATH", joined)
        // No HYB_ALLOW_TOOLCHAIN_DRIFT here: these tests exercise the real gate.
        .env_remove("HYB_ALLOW_TOOLCHAIN_DRIFT")
        .current_dir(cwd)
        .output()
        .expect("run the actual hyb build CLI")
}

fn assert_no_pin_error(output: &Output) {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !text.contains("HYB-E9001"),
        "a bridge-free build must not report a toolchain pin: {text}"
    );
}

#[test]
fn bridge_free_build_ignores_an_off_pin_rustc() {
    let dir = scratch("bridge-free");
    fs::write(dir.join("hello.hy"), "fn main() {\n    fmt.println(\"hi\")\n}\n").unwrap();
    let shim = shims(true);
    let out = build_with_path(&shim, "hello.hy", &dir, "hello");
    assert_eq!(
        out.status.code(),
        Some(0),
        "bridge-free build must succeed with an off-pin rustc: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_no_pin_error(&out);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&shim);
}

#[test]
fn bridge_free_build_still_works_with_no_rustc_on_path() {
    // The release-matrix contract (row 11/16): rustc legitimately absent.
    let dir = scratch("no-rustc");
    fs::write(dir.join("hello.hy"), "fn main() {\n    fmt.println(\"hi\")\n}\n").unwrap();
    let shim = shims(false);
    let out = build_with_path(&shim, "hello.hy", &dir, "hello");
    assert_eq!(
        out.status.code(),
        Some(0),
        "rustless bridge-free build must keep working: stdout={}, stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_no_pin_error(&out);
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&shim);
}

#[test]
fn bridge_build_that_must_rebuild_the_staticlib_still_enforces_the_rust_pin() {
    // Point the runtime at a copy with NO staticlib, so ensure_staticlib has to
    // run cargo — the one step the rust pin governs. The shim cargo would fail
    // on its own; the test asserts the pin refuses FIRST, naming the pin.
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("resolve the repo root");
    let runtime = repo.join("runtime");
    if !runtime.is_dir() {
        eprintln!("skipping: no runtime/ in this checkout");
        return;
    }
    let bare = scratch("runtime-nolib");
    copy_dir(&runtime, &bare);
    remove_staticlibs(&bare);

    let dir = scratch("bridge");
    let program = repo.join("examples/10-rust-crate-import/program.hy");
    if !program.is_file() {
        eprintln!("skipping: no bridge example in this checkout");
        return;
    }
    let shim = shims(true);
    let path = std::env::var_os("PATH").unwrap_or_default();
    let mut joined = shim.as_os_str().to_owned();
    joined.push(":");
    joined.push(&path);
    let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["build", program.to_str().unwrap(), "-o", "bridge-app"])
        .env("PATH", joined)
        .env("HYB_RUNTIME_DIR", &bare)
        .env_remove("HYB_ALLOW_TOOLCHAIN_DRIFT")
        .current_dir(&dir)
        .output()
        .expect("run the actual hyb build CLI");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        text.contains("HYB-E9001") && text.contains(SHIM_RUST_VERSION),
        "a staticlib rebuild must still enforce the rust pin, naming the found version: {text}"
    );
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&bare);
    let _ = fs::remove_dir_all(&shim);
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read runtime dir").flatten() {
        // Never copy build trees: the test only needs the layout and sources.
        if entry.file_name() == "target" {
            continue;
        }
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            fs::create_dir_all(&target).unwrap();
            copy_dir(&entry.path(), &target);
        } else {
            let _ = fs::copy(entry.path(), target);
        }
    }
}

fn remove_staticlibs(dir: &Path) {
    for entry in fs::read_dir(dir).expect("read copy").flatten() {
        let path = entry.path();
        if path.is_dir() {
            remove_staticlibs(&path);
        } else {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if name.starts_with("libhyrt") || name.starts_with("hyrt.lib") {
                let _ = fs::remove_file(&path);
            }
        }
    }
}
