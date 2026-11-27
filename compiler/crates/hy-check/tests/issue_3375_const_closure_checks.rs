//! issue_3375_const_closure_checks.rs
//!
//! Regression test for issue #3375:
//! Closures inside package-level const declarations must be checked for
//! Tier-0 concurrency, ownership, const-write, and managed checks.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("parse error");
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
fn test_reject_task_spawn_in_const_closure_hyb_e3001() {
    let src = r#"
package main

const runners = []fn(){fn() {
    m := map[string]int{"x": 0}
    done := make(chan bool, 2)
    task writer(m, done)
    task writer(m, done)
    <-done
    <-done
}}

fn writer(m map[string]int, done chan bool) {
    for i in 0..10000 {
        m["x"] = i
    }
    done <- true
}

fn main() {
    run := runners[0]
    run()
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3001".to_string()),
        "Expected HYB-E3001, got: {:?}",
        error_codes(&r)
    );
}

#[test]
fn test_reject_const_write_in_const_closure_hyb_e3006() {
    let src = r#"
package main

const shared = map[string]int{"k": 1}

const mutators = []fn(){fn() {
    for i in 0..10000 {
        shared["k"] = i
    }
}}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E3006".to_string()),
        "Expected HYB-E3006, got: {:?}",
        error_codes(&r)
    );
}

#[test]
fn test_reject_close_receive_only_and_defer_in_const_closure() {
    let src = r#"
package main

struct holder {
    run fn(ro <-chan int)
}

const h = holder{
    run: fn(ro <-chan int) {
        close(ro)
        defer 42
    },
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E3009".to_string()),
        "Expected HYB-E3009, got: {:?}",
        codes
    );
    assert!(
        codes.contains(&"HYB-E2001".to_string()),
        "Expected HYB-E2001, got: {:?}",
        codes
    );
}

#[test]
fn test_reject_post_move_write_and_non_owned_move_in_const_closure() {
    let src = r#"
package main

struct worker {
    run fn(m map[string]int, ch chan map[string]int)
}

const w = worker{
    run: fn(m map[string]int, ch chan map[string]int) {
        local_m := map[string]int{"a": 1}
        ch <- move local_m
        local_m["a"] = 2
        ch <- move m
    },
}

fn main() {}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "Expected HYB-E3003, got: {:?}",
        codes
    );
    assert!(
        codes.contains(&"HYB-E3004".to_string()),
        "Expected HYB-E3004, got: {:?}",
        codes
    );
}

#[test]
fn test_accept_pure_const_closures() {
    let src = r#"
package main

const ops = []fn(int) int{
    fn(x int) int {
        return x + 1
    },
    fn(x int) int {
        return x * 2
    },
}

fn main() {
    _ = ops[0](10)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "Diagnostics: {:?}", r.diagnostics);
}
