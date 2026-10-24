//! multivalue_task_forwarding_1174.rs — regression tests for issue #1174:
//! multi-value call forwarding at task spawn (`task worker(pair(m))`) only checked
//! the first return value, letting non-sendable maps/slices/pointers cross unchecked
//! (P0 data race).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_1174.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn warning_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn task_spawn_multivalue_forwarding_rejects_nonsendable_second_return_1174() {
    let src = "\
fn pair(m map[string]int) (int, map[string]int) {
    return 1, m
}

fn worker(n int, m map[string]int) {
    m[\"worker\"] = n
}

fn main() {
    m := map[string]int{\"init\": 0}
    task worker(pair(m))
    m[\"main\"] = 2
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "multi-value call forwarding with non-sendable second return must reject (issue #1174)"
    );
    assert!(
        errs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        errs
    );
}

#[test]
fn task_spawn_try_forwarding_rejects_nonsendable_second_return_1174() {
    let src = "\
fn pair_try(m map[string]int) (int, map[string]int, error) {
    return 1, m, nil
}

fn worker(n int, m map[string]int) {
    m[\"worker\"] = n
}

fn main() error {
    m := map[string]int{\"init\": 0}
    task worker(pair_try(m)?)
    m[\"main\"] = 2
    return nil
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "try forwarding with non-sendable second return must reject (issue #1174)"
    );
    assert!(
        errs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        errs
    );
}

#[test]
fn task_spawn_method_forwarding_rejects_nonsendable_second_return_1174() {
    let src = "\
struct box_val {}

fn (b box_val) pair(m map[string]int) (int, map[string]int) {
    return 1, m
}

fn worker(n int, m map[string]int) {
    m[\"worker\"] = n
}

fn main() {
    m := map[string]int{\"init\": 0}
    b := box_val{}
    task worker(b.pair(m))
    m[\"main\"] = 2
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "method forwarding with non-sendable second return must reject (issue #1174)"
    );
    assert!(
        errs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        errs
    );
}

#[test]
fn task_spawn_closure_forwarding_rejects_nonsendable_second_return_1174() {
    let src = "\
fn worker(n int, m map[string]int) {
    m[\"worker\"] = n
}

fn main() {
    m := map[string]int{\"init\": 0}
    f := fn(m map[string]int) (int, map[string]int) {
        return 1, m
    }
    task worker(f(m))
    m[\"main\"] = 2
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "closure forwarding with non-sendable second return must reject (issue #1174)"
    );
    assert!(
        errs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001, got {:?}",
        errs
    );
}

#[test]
fn task_spawn_multivalue_forwarding_rejects_unproven_second_return_e3005_1174() {
    let src = "\
fn pair_any(a any) (int, any) {
    return 1, a
}

fn worker(n int, a any) {}

fn main() {
    task worker(pair_any(1))
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "unproven return shape at task boundary must fail closed with HYB-E3005"
    );
    assert!(
        errs.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005, got {:?}",
        errs
    );
}

#[test]
fn task_spawn_safe_multivalue_forwarding_is_accepted_1174() {
    let src = "\
fn pair_safe() (int, string) {
    return 1, \"hello\"
}

fn worker_safe(n int, s string) {}

fn main() {
    task worker_safe(pair_safe())
}
";
    let r = check(src);
    let errs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "safe multi-value forwarding must accept with 0 errors"
    );
    assert!(errs.is_empty(), "expected 0 errors, got {:?}", errs);
}

#[test]
fn task_spawn_multivalue_forwarding_managed_emits_e8001_1174() {
    let src = "\
fn pair(m map[string]int) (int, map[string]int) {
    return 1, m
}

fn worker(n int, m map[string]int) {
    m[\"worker\"] = n
}

fn main() {
    m := map[string]int{\"init\": 0}
    managed(\"explicit bypass for multi-return\", \"other\") {
        task worker(pair(m))
    }
}
";
    let r = check(src);
    let errs = error_codes(&r);
    let warns = warning_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "managed block suppresses HYB-E3001 error and accepts"
    );
    assert!(errs.is_empty(), "expected 0 errors, got {:?}", errs);
    assert!(
        warns.iter().any(|c| c == "HYB-E8001"),
        "expected HYB-E8001 warning, got {:?}",
        warns
    );
}
