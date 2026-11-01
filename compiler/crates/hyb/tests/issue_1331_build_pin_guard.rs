//! Regression tests for #1331: `hyb/build.rs` compile-time pin drift guard
//! honors `HYB_ALLOW_TOOLCHAIN_DRIFT=1`, normalizes empty/whitespace
//! `HYB_PINNED_RUST` to `DEFAULT_PIN` ("1.99.0"), and tracks `RUSTC` and
//! `HYB_ALLOW_TOOLCHAIN_DRIFT` via `cargo:rerun-if-env-changed`.

#[path = "../build.rs"]
mod build_script;

#[test]
fn empty_or_whitespace_pinned_rust_falls_back_to_default_pin() {
    assert_eq!(build_script::resolve_pinned_rust(None), build_script::DEFAULT_PIN);
    assert_eq!(build_script::resolve_pinned_rust(Some("")), build_script::DEFAULT_PIN);
    assert_eq!(build_script::resolve_pinned_rust(Some("   	
  ")), build_script::DEFAULT_PIN);
    assert_eq!(build_script::resolve_pinned_rust(Some(" 1.98.0 ")), "1.98.0");
}

#[test]
fn allow_toolchain_drift_recognizes_explicit_one_only() {
    assert!(build_script::allow_toolchain_drift(Some("1")));
    assert!(build_script::allow_toolchain_drift(Some(" 1 ")));
    assert!(!build_script::allow_toolchain_drift(None));
    assert!(!build_script::allow_toolchain_drift(Some("")));
    assert!(!build_script::allow_toolchain_drift(Some("0")));
    assert!(!build_script::allow_toolchain_drift(Some("true")));
}

#[test]
fn build_script_emits_required_rerun_and_rustc_env_directives() {
    let build_src = include_str!("../build.rs");
    assert!(
        build_src.contains("cargo:rerun-if-env-changed=HYB_ALLOW_TOOLCHAIN_DRIFT"),
        "build.rs must track HYB_ALLOW_TOOLCHAIN_DRIFT changes"
    );
    assert!(
        build_src.contains("cargo:rerun-if-env-changed=RUSTC"),
        "build.rs must track RUSTC changes"
    );
    assert!(
        build_src.contains("cargo:rustc-env=HYB_PINNED_RUST="),
        "build.rs must emit normalized HYB_PINNED_RUST for main.rs"
    );
}
