//! Regression tests for Wave 24:
//! Curated foreign package function calls (`sync.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity and non-callable checks on package `sync`, invocations on struct types
//! (`sync.mutex()`, `sync.rw_mutex()`, `sync.wait_group()`, `sync.pool()`, `sync.once()`, `sync.cond()`, `sync.map()`),
//! or invocations with invalid arity (`sync.new_cond()`),
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
fn test_sync_non_callable_types_rejected() {
    // sync.mutex
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.mutex()
}
"#,
    );

    // sync.rw_mutex
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.rw_mutex()
}
"#,
    );

    // sync.wait_group
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.wait_group()
}
"#,
    );

    // sync.pool
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.pool()
}
"#,
    );

    // sync.once
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.once()
}
"#,
    );

    // sync.cond
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.cond()
}
"#,
    );

    // sync.map
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.map()
}
"#,
    );
}

#[test]
fn test_sync_new_cond_call_arity_rejected() {
    // sync.new_cond takes 1 argument, called with 0
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    _ = sync.new_cond()
}
"#,
    );

    // sync.new_cond takes 1 argument, called with 2
    rejects_e1002(
        r#"
package main

import "sync"

fn test() {
    var m sync.mutex
    _ = sync.new_cond(&m, &m)
}
"#,
    );
}

#[test]
fn test_sync_positive_controls_accepted() {
    accepts(
        r#"
package main

import "sync"

fn test() {
    var m sync.mutex
    _ = sync.new_cond(&m)
}
"#,
    );
}
