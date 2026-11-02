//! Regression test for issue #3375:
//! Closures inside package-level const composite literals bypass Tier-0
//! concurrency, ownership, const-write, and managed checks.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("test_3375.hy", src).expect("fixture must parse");
    let result = check_file(&ast);
    if let Some(code) = expected {
        assert_eq!(
            result.verdict,
            Verdict::Reject,
            "expected reject with {code}, got: {:#?}",
            result.diagnostics
        );
        assert!(
            result.diagnostics.iter().any(|d| d.code == code),
            "expected {code}, got {:#?}",
            result.diagnostics
        );
    } else {
        assert_eq!(
            result.verdict,
            Verdict::Accept,
            "expected accept, got diagnostics: {:#?}",
            result.diagnostics
        );
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }
}

#[test]
fn const_composite_closure_task_sendable_reject() {
    verify(
        r#"
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

fn main() {}
"#,
        Some("HYB-E3001"),
    );
}

#[test]
fn const_composite_closure_const_write_reject() {
    verify(
        r#"
package main

const shared = map[string]int{"k": 1}

const mutators = []fn(){fn() {
    for i in 0..10000 {
        shared["k"] = i
    }
}}

fn reader(done chan bool) {
    for _ in 0..10000 {
        _ = shared["k"]
    }
    done <- true
}

fn main() {}
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn const_composite_closure_chan_sendable_reject() {
    verify(
        r#"
package main

struct holder {
    run fn(ch chan map[string]int)
}

const h = holder{
    run: fn(ch chan map[string]int) {
        local_m := map[string]int{"a": 1}
        ch <- local_m
    },
}

fn main() {}
"#,
        Some("HYB-E3002"),
    );
}

#[test]
fn const_composite_closure_move_freezes_reject() {
    verify(
        r#"
package main

struct holder {
    run fn(ch chan map[string]int)
}

const h = holder{
    run: fn(ch chan map[string]int) {
        local_m := map[string]int{"a": 1}
        ch <- move local_m
        local_m["a"] = 2
    },
}

fn main() {}
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn const_composite_closure_close_recv_chan_reject() {
    verify(
        r#"
package main

struct holder {
    run fn(ro <-chan int)
}

const h = holder{
    run: fn(ro <-chan int) {
        close(ro)
    },
}

fn main() {}
"#,
        Some("HYB-E3009"),
    );
}

#[test]
fn const_composite_closure_safe_accept() {
    verify(
        r#"
package main

const handlers = []fn(int) int{
    fn(x int) int {
        return x + 1
    },
    fn(x int) int {
        return x * 2
    },
}

fn main() {}
"#,
        None,
    );
}
