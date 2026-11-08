//! Regression tests for Wave 25:
//! Curated foreign package function calls (`time.*`) must enforce call arity
//! constraints and non-callable member checks during `hyb check` (HYB-E1002, Row 13 BUILDABLE).
//!
//! Without arity and non-callable checks on package `time`, invocations on constants and types
//! (`time.second()`, `time.millisecond()`, `time.duration()`, `time.time()`, `time.rfc3339()`),
//! or invocations with invalid arity (`time.now(1)`, `time.sleep()`, `time.since()`),
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
fn test_time_non_callable_members_rejected() {
    // Duration constants
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.second()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.millisecond()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.nanosecond()
}
"#,
    );

    // Format constants
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.rfc3339()
}
"#,
    );

    // Types
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.duration()
}
"#,
    );

    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.time()
}
"#,
    );
}

#[test]
fn test_time_call_arity_rejected() {
    // time.now takes 0 args, called with 1
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.now(123)
}
"#,
    );

    // time.sleep takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    time.sleep()
}
"#,
    );

    // time.sleep takes 1 arg, called with 2
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    time.sleep(1, 2)
}
"#,
    );

    // time.since takes 1 arg, called with 0
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.since()
}
"#,
    );

    // time.unix takes 2 args, called with 1
    rejects_e1002(
        r#"
package main

import "time"

fn test() {
    _ = time.unix(1234567890)
}
"#,
    );
}

#[test]
fn test_time_positive_controls_accepted() {
    accepts(
        r#"
package main

import "time"

fn test() {
    t := time.now()
    _ = time.since(t)
    _ = time.until(t)
    time.sleep(time.seconds(1))
    _ = time.unix(100, 200)
}
"#,
    );
}
