//! Issue #4368 — a deferred argument's storage stored into a FIELD or INDEX
//! projection keeps the deferred call reading it at function exit. Both
//! `update_pre_defer_peers` and `update_post_defer_watch` used to match only
//! `PlaceKind::Ident`, so `b.m = m` / `b["k"] = m` left the root object out of
//! the defer peer/watch sets and `task worker(move b)` was accepted while the
//! deferred call raced the background task on the same map.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e3003(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", codes);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        codes
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics
            .iter()
            .all(|d| d.severity != Severity::Error),
        "expected no errors, got {:?}",
        r.diagnostics
    );
}

#[test]
fn field_store_after_defer_then_rebind_and_task_move_rejects_4368() {
    // The exact issue #4368 reproducer: `defer report(m)` captures m's map;
    // `b.m = m` puts that same storage on b; rebinding m severs the name but
    // not b.m; moving b into a task must therefore be rejected.
    assert_rejects_e3003(
        "struct box {\n\
           m map[string]int\n\
         }\n\
         fn worker(b box) {\n\
           b.m[\"k\"] = 99\n\
         }\n\
         fn report(m map[string]int) {\n\
           m[\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           b := box{m: map[string]int{}}\n\
           defer report(m)\n\
           b.m = m\n\
           m = map[string]int{\"k\": 2}\n\
           task worker(move b)\n\
         }",
    );
}

#[test]
fn index_store_after_defer_then_rebind_and_task_move_rejects_4368() {
    // The index twin: `b["k"] = m` stores the captured map inside b.
    assert_rejects_e3003(
        "fn worker(b map[string]map[string]int) {\n\
           b[\"k\"][\"x\"] = 99\n\
         }\n\
         fn report(m map[string]int) {\n\
           m[\"x\"] = 1\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"x\": 0}\n\
           b := map[string]map[string]int{}\n\
           defer report(m)\n\
           b[\"k\"] = m\n\
           m = map[string]int{\"x\": 2}\n\
           task worker(move b)\n\
         }",
    );
}

#[test]
fn field_store_before_defer_then_task_move_rejects_4368() {
    // Pre-defer peer tracking: b already carries m's storage when the defer
    // evaluates its argument, so moving b is a transfer of the deferred map.
    assert_rejects_e3003(
        "struct box {\n\
           m map[string]int\n\
         }\n\
         fn worker(b box) {\n\
           b.m[\"k\"] = 99\n\
         }\n\
         fn report(m map[string]int) {\n\
           m[\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           b := box{m: map[string]int{}}\n\
           b.m = m\n\
           defer report(m)\n\
           task worker(move b)\n\
         }",
    );
}

#[test]
fn field_store_of_unrelated_map_stays_accept_4368() {
    // Positive control: b.m shares m's storage, but the deferred call reads a
    // DIFFERENT map, so moving b does not race the deferred read.
    assert_accepts(
        "struct box {\n\
           m map[string]int\n\
         }\n\
         fn worker(b box) {\n\
           b.m[\"k\"] = 99\n\
         }\n\
         fn report(m map[string]int) {\n\
           m[\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           other := map[string]int{\"k\": 7}\n\
           b := box{m: map[string]int{}}\n\
           defer report(other)\n\
           b.m = m\n\
           task worker(move b)\n\
         }",
    );
}

#[test]
fn field_store_of_fresh_map_after_defer_stays_accept_4368() {
    // Second control: the field store does not alias the deferred map.
    assert_accepts(
        "struct box {\n\
           m map[string]int\n\
         }\n\
         fn worker(b box) {\n\
           b.m[\"k\"] = 99\n\
         }\n\
         fn report(m map[string]int) {\n\
           m[\"k\"] = 1\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           b := box{m: map[string]int{}}\n\
           defer report(m)\n\
           b.m = map[string]int{\"k\": 3}\n\
           task worker(move b)\n\
         }",
    );
}
