//! #3064 - the predeclared error type had no OpKind.
//!
//! error fell through kind_of_named to Unknown, and kinds_conflict is
//! fail-open on Unknown, so every operand check involving an error value
//! passed. Each rejected case below is accepted by hyb check on main and
//! fails go build: a Row 13 BUILDABLE soundness hole.
//!
//! Reproduced as real go build failures first, on the pinned toolchain:
//!   cannot use err (variable of interface type error) as int value ...
//!   cannot use 42 (constant of type int) as error value ...
//!
//! The controls are buildable and must stay accepted, so the rule cannot
//! be widened into one that refuses good programs.

use std::path::PathBuf;
use std::process::Command;

fn checker() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/hyb")
}

fn check(src: &str) -> (bool, String) {
    let dir = std::env::temp_dir().join(format!("hyb3064-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("case.hy");
    std::fs::write(&file, src).unwrap();
    let out = Command::new(checker()).arg("check").arg(&file).output().unwrap();
    let mut msg = String::from_utf8_lossy(&out.stdout).to_string();
    msg.push_str(&String::from_utf8_lossy(&out.stderr));
    (out.status.success(), msg)
}

#[test]
fn error_argument_into_int_param_is_rejected() {
    let (ok, msg) = check("fn take_int(x int) {}\n\nfn main() {\n    var err error\n    take_int(err)\n}\n");
    assert!(!ok, "error passed to an int param was accepted");
    assert!(msg.contains("HYB-E1002"), "expected HYB-E1002, got: {msg}");
}

#[test]
fn error_value_returned_from_int_fn_is_rejected() {
    let (ok, msg) = check("fn f() int {\n    var err error\n    return err\n}\n\nfn main() { _ = f() }\n");
    assert!(!ok, "returning an error from an int fn was accepted");
    assert!(msg.contains("HYB-E1002"), "expected HYB-E1002, got: {msg}");
}

#[test]
fn int_argument_into_error_param_is_rejected() {
    let (ok, msg) = check("fn take_error(e error) {}\n\nfn main() {\n    take_error(42)\n}\n");
    assert!(!ok, "an int passed to an error param was accepted");
    assert!(msg.contains("HYB-E1002"), "expected HYB-E1002, got: {msg}");
}

#[test]
fn error_passed_between_error_positions_stays_accepted() {
    let (ok, msg) = check("fn take_error(e error) {}\n\nfn f() error {\n    var err error\n    take_error(err)\n    return err\n}\n\nfn main() { _ = f() }\n");
    assert!(ok, "the all-error control was rejected: {msg}");
}
