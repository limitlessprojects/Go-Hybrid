//! The `built <out> (go <version>)` banner must name the Go toolchain that
//! actually ran `go build`, not the compile-time pin. With
//! HYB_ALLOW_TOOLCHAIN_DRIFT=1 the two differ, and a banner that always says
//! the pin (`go1.26.9`) misreports the build's provenance.

#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

/// A fake `go` first on PATH: reports `go9.9.9` and "builds" by touching the
/// `-o` target, so the test needs no real toolchain and cannot drift with one.
const FAKE_GO: &str = r#"#!/bin/sh
if [ "$1" = version ]; then
    echo "go version go9.9.9 testos/testarch"
    exit 0
fi
out=""
while [ $# -gt 0 ]; do
    if [ "$1" = "-o" ]; then out="$2"; fi
    shift
done
[ -n "$out" ] && : > "$out"
exit 0
"#;

#[test]
fn banner_reports_the_go_that_built_not_the_pin() {
    let root = std::env::temp_dir().join(format!("hyb-banner-go-{}", std::process::id()));
    let bin: PathBuf = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let go = bin.join("go");
    fs::write(&go, FAKE_GO).unwrap();
    fs::set_permissions(&go, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(root.join("good.hy"), "fn main() {}\n").unwrap();

    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let out = Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(["build", "good.hy", "-o", "app"])
        .env("HYB_ALLOW_TOOLCHAIN_DRIFT", "1")
        .env("PATH", path)
        .current_dir(&root)
        .output()
        .expect("run hyb build");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    fs::remove_dir_all(&root).unwrap();

    assert_eq!(out.status.code(), Some(0), "stdout={stdout} stderr={stderr}");
    assert!(
        stdout.contains("built app (go go9.9.9)"),
        "banner must name the go that built: {stdout}"
    );
}
