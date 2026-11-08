//! Regression tests for #4100:
//! Curated foreign receiver methods (`sync.mutex`, `sync.rwmutex`, `sync.wait_group`,
//! `sync.once`, `time.ticker`, `time.timer`, `strings.builder`, `bytes.buffer`) must
//! enforce call arity and operand constraints during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity checks on foreign receiver methods, calls with too few or too many arguments
//! passed `hyb check` with 0 diagnostics and subsequently failed downstream `go build`.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_sync_mutex_arity_rejection() {
    // sync.mutex.lock, unlock, try_lock take 0 arguments
    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut mu sync.mutex;
    mu.lock(1);
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut mu sync.mutex;
    mu.unlock(1, 2);
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut mu sync.mutex;
    mu.try_lock(true);
}
"#,
    );
}

#[test]
fn test_sync_waitgroup_arity_and_operand_rejection() {
    // wg.add requires exactly 1 integer argument
    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut wg sync.wait_group;
    wg.add();
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut wg sync.wait_group;
    wg.add(1, 2);
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut wg sync.wait_group;
    wg.add("invalid");
}
"#,
    );

    // wg.done and wg.wait take 0 arguments
    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut wg sync.wait_group;
    wg.done(42);
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut wg sync.wait_group;
    wg.wait("timeout");
}
"#,
    );
}

#[test]
fn test_sync_once_arity_rejection() {
    // once.do requires exactly 1 function argument
    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut once sync.once;
    once.do();
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "sync"

fn main() {
    let mut once sync.once;
    once.do(fn() {}, 42);
}
"#,
    );
}

#[test]
fn test_time_ticker_and_timer_arity_rejection() {
    // ticker.stop takes 0 arguments
    rejects_e1002(
        r#"
package main

import "time"

fn test(t *time.ticker) {
    t.stop(1);
}
"#,
    );

    // ticker.reset requires exactly 1 duration argument
    rejects_e1002(
        r#"
package main

import "time"

fn test(t *time.ticker) {
    t.reset();
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "time"

fn test(t *time.ticker) {
    t.reset("10s");
}
"#,
    );

    // timer.stop takes 0 arguments
    rejects_e1002(
        r#"
package main

import "time"

fn test(t *time.timer) {
    t.stop(1);
}
"#,
    );

    // timer.reset requires 1 duration argument
    rejects_e1002(
        r#"
package main

import "time"

fn test(t *time.timer) {
    t.reset();
}
"#,
    );
}

#[test]
fn test_valid_foreign_receiver_methods_accepted() {
    accepts(
        r#"
package main

import "sync"
import "time"

fn main() {
    let mut mu sync.mutex;
    mu.lock();
    mu.unlock();

    let mut wg sync.wait_group;
    wg.add(1);
    wg.done();
    wg.wait();

    let mut once sync.once;
    once.do(fn() {});
}
"#,
    );
}
