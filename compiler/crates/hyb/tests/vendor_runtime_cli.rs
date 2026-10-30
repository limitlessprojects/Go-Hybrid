//! Integration tests for `hyb vendor-runtime` CLI option validation (issue #1319).

use std::process::{Command, Output};

fn vendor(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hyb"))
        .arg("vendor-runtime")
        .args(args)
        .output()
        .expect("spawn hyb vendor-runtime")
}

#[test]
fn vendor_runtime_rejects_unknown_flags_and_invalid_target_dir_args() {
    for (args, expected) in [
        (vec![], "usage: hyb vendor-runtime"),
        (vec!["-h"], "unknown vendor-runtime option: -h"),
        (vec!["--unknown", "out"], "unknown vendor-runtime option: --unknown"),
        (vec!["--target-dir"], "--target-dir requires a value"),
        (
            vec!["--target-dir", "--bad"],
            "--target-dir requires a value, not an option",
        ),
        (
            vec!["--target-dir", "a", "--target-dir", "b"],
            "target directory may only be supplied once",
        ),
        (
            vec!["a", "b"],
            "vendor-runtime expects a single target directory",
        ),
    ] {
        let out = vendor(&args);
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
