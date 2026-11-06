//! Regression tests for foreign method surface gating on `sync.cond`, `sync.pool`,
//! `sync.Map`, `time.ticker`, and `time.timer` handles (`HYB-E1002`, Row 13 BUILDABLE).
//!
//! Under GUARANTEES.md Row 13 (BUILDABLE), every program accepted by `hyb check`
//! must emit valid Go that builds with `go build`.
//! Calling a phantom or non-existent method on these standard library handles passed
//! `hyb check` with 0 diagnostics and died at `go build` with
//! `type ... has no field or method ...`.

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

fn rejects_e1002(src: &str, expected_substring: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.contains(&"HYB-E1002".to_string()),
        "must emit HYB-E1002, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.message.contains(expected_substring)),
        "diagnostic message must contain {:?}, got: {:?}",
        expected_substring,
        r.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "must accept with 0 errors, got: {:?} for\n{}",
        codes,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn test_sync_cond_phantom_method_rejects() {
    let src = r#"
import "sync"

struct state {
    cond sync.cond
}

fn main() {
    var s state
    s.cond.totally_fake()
}
"#;
    rejects_e1002(src, "sync.cond");
}

#[test]
fn test_sync_pool_phantom_method_rejects() {
    let src = r#"
import "sync"

struct state {
    p sync.pool
}

fn main() {
    var s state
    s.p.invalid_method()
}
"#;
    rejects_e1002(src, "sync.pool");
}

#[test]
fn test_sync_map_phantom_method_rejects() {
    let src = r#"
import "sync"

struct state {
    m sync.Map
}

fn main() {
    var s state
    s.m.bogus_call()
}
"#;
    rejects_e1002(src, "sync.Map");
}

#[test]
fn test_time_ticker_phantom_method_rejects() {
    let src = r#"
import "time"

struct state {
    tk time.ticker
}

fn main() {
    var s state
    s.tk.pause()
}
"#;
    rejects_e1002(src, "time.ticker");
}

#[test]
fn test_time_timer_phantom_method_rejects() {
    let src = r#"
import "time"

struct state {
    tm time.timer
}

fn main() {
    var s state
    s.tm.pause()
}
"#;
    rejects_e1002(src, "time.timer");
}

#[test]
fn test_real_methods_on_sync_and_time_handles_accept() {
    let src = r#"
import "sync"
import "time"

struct state {
    cond   sync.cond
    pool   sync.pool
    smap   sync.Map
    ticker time.ticker
    timer  time.timer
}

fn main() {
    var s state
    s.cond.signal()
    s.cond.broadcast()
    _ = s.pool.get()
    s.pool.put(1)
    s.smap.store("k", "v")
    s.smap.delete("k")
    s.ticker.stop()
    s.timer.stop()
}
"#;
    accepts(src);
}