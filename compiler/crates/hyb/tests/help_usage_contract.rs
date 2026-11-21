//! The top-level `hyb` usage is a discoverability surface, and it had drifted:
//! it advertised only `hyb build <file.hy> -o <bin-path> [--emit-go] [--race]`,
//! omitting package mode (`--package/--out-dir/--binding-prefix`), proof
//! emission (`--emit-proof`), and the managed-block policy flag
//! (`--deny-managed`) — all of which the CLI accepts and CI exercises.
//!
//! The help now renders the canonical `BUILD_USAGE` string shared with the
//! flag-validation errors. These tests execute the real binary so a future
//! duplicate help line cannot quietly drop a mode again.

use std::process::{Command, Output};

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hyb"))
        .args(args)
        .output()
        .expect("run actual hyb CLI")
}

#[test]
fn top_level_help_lists_every_subcommand() {
    let output = run(&[]);
    assert_eq!(
        output.status.code(),
        Some(2),
        "no-argument usage is a usage error (stdout={}, stderr={})",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8(output.stderr).unwrap();
    for subcommand in [
        "hyb check",
        "hyb generate",
        "hyb vendor-runtime",
        "hyb explain",
        "hyb build",
        "hyb bench-overhead",
    ] {
        assert!(stderr.contains(subcommand), "help omits `{subcommand}`:\n{stderr}");
    }
}

#[test]
fn top_level_help_documents_both_build_modes_and_every_build_flag() {
    let stderr = String::from_utf8(run(&[]).stderr).unwrap();
    // Package mode is the only way to transpile a whole directory into an
    // existing Go module; `--emit-proof` and `--deny-managed` are policy /
    // evidence features. All three were accepted but undiscoverable here.
    for needle in [
        "--package",
        "--out-dir",
        "--binding-prefix",
        "--emit-go",
        "--emit-proof",
        "--race",
        "--deny-managed",
    ] {
        assert!(
            stderr.contains(needle),
            "top-level help omits build flag `{needle}`:\n{stderr}"
        );
    }
}

#[test]
fn help_and_build_usage_agree_on_the_single_file_form() {
    let help = String::from_utf8(run(&[]).stderr).unwrap();
    let build_usage = String::from_utf8(run(&["build"]).stderr).unwrap();
    let build_line = build_usage
        .lines()
        .find(|line| line.starts_with("usage: hyb build "))
        .expect("build usage line");
    assert!(
        help.contains(build_line.trim_start_matches("usage: ")),
        "top-level help and `hyb build` usage disagree:\nhelp:\n{help}\nbuild:\n{build_usage}"
    );
}
