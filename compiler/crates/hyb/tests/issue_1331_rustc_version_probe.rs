//! Regression tests for #1331: `hyb/build.rs` fails open when the `rustc --version`
//! probe exits non-zero or emits non-`rustc` diagnostic text on stdout.

#[path = "../build.rs"]
mod build_script;

#[test]
fn parse_rustc_version_accepts_valid_rustc_banner() {
    assert_eq!(
        build_script::parse_rustc_version(true, "rustc 1.99.0 (abcdef123 2026-05-01)
"),
        Some("1.99.0".to_string())
    );
    assert_eq!(
        build_script::parse_rustc_version(true, "rustc 1.94.1-nightly (0123456 2026-04-01)"),
        Some("1.94.1-nightly".to_string())
    );
}

#[test]
fn parse_rustc_version_fails_open_on_nonzero_status_or_non_rustc_banner() {
    // Non-zero exit status must fail open even if stdout has words.
    assert_eq!(
        build_script::parse_rustc_version(false, "error: no default toolchain configured
"),
        None
    );
    assert_eq!(
        build_script::parse_rustc_version(false, "rustc 1.99.0 (broken shim)
"),
        None
    );
    // Non-rustc banner on stdout must fail open instead of taking the 2nd token.
    assert_eq!(
        build_script::parse_rustc_version(true, "error: rustup could not choose a version
"),
        None
    );
    assert_eq!(build_script::parse_rustc_version(true, ""), None);
    assert_eq!(build_script::parse_rustc_version(true, "rustc   
"), None);
}
