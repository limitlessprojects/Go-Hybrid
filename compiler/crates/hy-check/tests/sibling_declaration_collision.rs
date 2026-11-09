//! sibling_declaration_collision.rs — Regression test suite for Issue #4222:
//! A `.hy` file that collides with a sibling `.go`/`.hy` top-level declaration
//! passes `hyb check` and breaks the package build after `hyb generate`.
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every accepted program must build.
//! Package-local collisions with sibling files must be rejected fail-closed as `HYB-E1002`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;
use std::fs;
use std::path::PathBuf;

fn create_temp_pkg(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hyb-sibling-test-{}-{}", prefix, std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn check_path(path: &std::path::Path) -> hy_check::CheckResult {
    let src = fs::read_to_string(path).expect("reading file");
    let ast = parse_file(&path.to_string_lossy(), &src).expect("parsing file");
    check_file(&ast)
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn test_reproduction_a_hy_collides_with_sibling_go() {
    let dir = create_temp_pkg("repro-a");
    let main_go = dir.join("main.go");
    let extra_hy = dir.join("extra.hy");

    fs::write(
        &main_go,
        "package main\n\nfunc helper(n int) int {\n    return n + 10\n}\n\nfunc main() {}\n",
    )
    .unwrap();

    fs::write(
        &extra_hy,
        "fn helper(n int) int {\n    return n + 1\n}\n\nfn extra(n int) int {\n    return n * 2\n}\n",
    )
    .unwrap();

    let res = check_path(&extra_hy);
    assert_eq!(res.verdict, Verdict::Reject, "expected rejection due to collision with sibling main.go");

    let codes = error_codes(&res);
    assert!(codes.contains(&"HYB-E1002".to_string()), "expected HYB-E1002, got: {:?}", codes);

    let msgs = error_messages(&res);
    assert!(
        msgs.iter().any(|m| m.contains("`helper` redeclared in this block") && m.contains("main.go")),
        "expected collision error naming helper and main.go, got: {:?}",
        msgs
    );

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_reproduction_b_two_hy_files_collide() {
    let dir = create_temp_pkg("repro-b");
    let a_hy = dir.join("a.hy");
    let b_hy = dir.join("b.hy");

    fs::write(
        &a_hy,
        "fn helper(n int) int {\n    return n + 1\n}\n",
    )
    .unwrap();

    fs::write(
        &b_hy,
        "fn helper(n int) int {\n    return n + 1\n}\n\nfn extra(n int) int {\n    return n * 2\n}\n",
    )
    .unwrap();

    // Both files must reject when checked individually
    let res_a = check_path(&a_hy);
    assert_eq!(res_a.verdict, Verdict::Reject, "expected a.hy to reject due to collision with sibling b.hy");
    assert!(error_codes(&res_a).contains(&"HYB-E1002".to_string()));

    let res_b = check_path(&b_hy);
    assert_eq!(res_b.verdict, Verdict::Reject, "expected b.hy to reject due to collision with sibling a.hy");
    assert!(error_codes(&res_b).contains(&"HYB-E1002".to_string()));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_control_distinct_names_accepted() {
    let dir = create_temp_pkg("control-distinct");
    let main_go = dir.join("main.go");
    let extra_hy = dir.join("extra.hy");

    fs::write(
        &main_go,
        "package main\n\nfunc go_worker() {}\n",
    )
    .unwrap();

    fs::write(
        &extra_hy,
        "fn hy_worker() int {\n    return 42\n}\n",
    )
    .unwrap();

    let res = check_path(&extra_hy);
    assert_eq!(res.verdict, Verdict::Accept, "expected distinct names to accept cleanly with 0 errors");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_control_method_names_not_colliding() {
    let dir = create_temp_pkg("control-method");
    let main_go = dir.join("main.go");
    let extra_hy = dir.join("extra.hy");

    // helper is a method on Receiver, NOT a package-level declaration
    fs::write(
        &main_go,
        "package main\n\ntype Receiver struct{}\n\nfunc (r Receiver) helper() {}\n",
    )
    .unwrap();

    fs::write(
        &extra_hy,
        "fn helper() int {\n    return 1\n}\n",
    )
    .unwrap();

    let res = check_path(&extra_hy);
    assert_eq!(res.verdict, Verdict::Accept, "methods on receiver must not collide with package-level functions");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_control_struct_fields_not_colliding() {
    let dir = create_temp_pkg("control-fields");
    let main_go = dir.join("main.go");
    let extra_hy = dir.join("extra.hy");

    // helper is a struct field, NOT a package-level declaration
    fs::write(
        &main_go,
        "package main\n\ntype S struct {\n    helper int\n}\n",
    )
    .unwrap();

    fs::write(
        &extra_hy,
        "fn helper() int {\n    return 1\n}\n",
    )
    .unwrap();

    let res = check_path(&extra_hy);
    assert_eq!(res.verdict, Verdict::Accept, "struct fields must not collide with package-level functions");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_type_and_const_collisions() {
    let dir = create_temp_pkg("type-const-collision");
    let types_go = dir.join("types.go");
    let app_hy = dir.join("app.hy");

    fs::write(
        &types_go,
        "package main\n\ntype config struct {\n    Port int\n}\n\nconst max_retries = 5\n",
    )
    .unwrap();

    fs::write(
        &app_hy,
        "struct config {\n    port int\n}\n\nconst max_retries = 10\n",
    )
    .unwrap();

    let res = check_path(&app_hy);
    assert_eq!(res.verdict, Verdict::Reject);
    let msgs = error_messages(&res);
    assert!(msgs.iter().any(|m| m.contains("`config` redeclared in this block")));
    assert!(msgs.iter().any(|m| m.contains("`max_retries` redeclared in this block")));

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_pub_fn_pascal_name_collision() {
    let dir = create_temp_pkg("pub-pascal-collision");
    let main_go = dir.join("main.go");
    let extra_hy = dir.join("extra.hy");

    // main.go declares PascalCase function Helper
    fs::write(
        &main_go,
        "package main\n\nfunc Helper() {}\n",
    )
    .unwrap();

    // extra.hy exports pub fn helper (which lowers to Helper in package mode)
    fs::write(
        &extra_hy,
        "pub fn helper() int {\n    return 42\n}\n",
    )
    .unwrap();

    let res = check_path(&extra_hy);
    assert_eq!(res.verdict, Verdict::Reject, "expected pub fn helper to collide with Go func Helper");
    let msgs = error_messages(&res);
    assert!(msgs.iter().any(|m| m.contains("Helper") && m.contains("main.go")));

    let _ = fs::remove_dir_all(&dir);
}
