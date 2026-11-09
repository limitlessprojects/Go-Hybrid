//! closure_call_arity_soundness.rs — tests for HYB-E1002 validation of
//! arity and argument types for immediately-invoked closure calls and task spawns.

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
fn inline_closure_too_many_args_rejects_e1002() {
    let src = "fn main() {\n    _ = (fn(x int) int { return x + 1 })(10, 20)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn inline_closure_too_few_args_rejects_e1002() {
    let src = "fn main() {\n    _ = (fn(x int, y int) int { return x + y })(10)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn inline_closure_arg_type_mismatch_rejects_e1002() {
    let src = "fn main() {\n    _ = (fn(x int) int { return x * 2 })(\"invalid\")\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn task_spawn_inline_closure_too_many_args_rejects_e1002() {
    let src = "fn main() {\n    managed(\"test\", \"actor\") {\n        task (fn(x int) { _ = x })(1, 2)\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn task_spawn_inline_closure_arg_type_mismatch_rejects_e1002() {
    let src = "fn main() {\n    managed(\"test\", \"actor\") {\n        task (fn(x int) { _ = x })(\"bad\")\n    }\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn valid_inline_closure_call_and_task_accepts() {
    let src = "fn main() {\n    a := (fn(x int) int { return x + 1 })(10)\n    managed(\"test\", \"actor\") {\n        task (fn(s string) { _ = s })(\"hello\")\n    }\n    _ = a\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
