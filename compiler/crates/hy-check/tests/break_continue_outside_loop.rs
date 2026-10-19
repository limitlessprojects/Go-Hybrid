//! break_continue_outside_loop.rs — tests for HYB-E1002 rejection of
//! plain `break` and `continue` statements outside of loops/selects.
//!
//! Go syntax rules:
//! - `break` is only permitted inside `for`, `switch`, or `select`.
//! - `continue` is only permitted inside `for`.
//! Outside these, `go build` fails with `syntax error: break is not in a loop`
//! or `syntax error: continue is not in a loop`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn toplevel_break_rejects_e1002() {
    let src = "fn main() {\n    break\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn toplevel_continue_rejects_e1002() {
    let src = "fn main() {\n    continue\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn if_break_outside_loop_rejects_e1002() {
    let src = "fn main() {\n    if true {\n        break\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn if_continue_outside_loop_rejects_e1002() {
    let src = "fn main() {\n    if true {\n        continue\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn closure_break_outside_loop_rejects_e1002() {
    let src = "fn main() {\n    f := fn() {\n        break\n    }\n    _ = f\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn closure_continue_outside_loop_rejects_e1002() {
    let src = "fn main() {\n    f := fn() {\n        continue\n    }\n    _ = f\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn closure_inside_loop_break_rejects_e1002() {
    // Loop outside does not enclose statements inside closure
    let src = "fn main() {\n    for {\n        f := fn() {\n            break\n        }\n        _ = f\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn select_continue_outside_loop_rejects_e1002() {
    let src = "fn main() {\n    select {\n        default:\n            continue\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn select_break_outside_loop_accepted() {
    // In Go, `break` inside select is legal (breaks out of select)
    let src = "fn main() {\n    select {\n        default:\n            break\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}

#[test]
fn loop_break_and_continue_accepted() {
    let src = "fn main() {\n    for {\n        if true {\n            continue\n        }\n        break\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}

#[test]
fn closure_loop_break_and_continue_accepted() {
    let src = "fn main() {\n    f := fn() {\n        for {\n            if true {\n                continue\n            }\n            break\n        }\n    }\n    _ = f\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(error_codes(&r).is_empty());
}
