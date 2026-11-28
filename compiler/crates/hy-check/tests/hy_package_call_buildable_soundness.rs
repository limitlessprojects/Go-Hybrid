//! Regression tests for Row 13 (BUILDABLE) soundness on `hy` runtime package calls:
//! The `hy` package call surface must be arity-checked and non-callable capability
//! types (`hy.send_cap`, `hy.recv_cap`, `hy.channel`) must be rejected at `hyb check` time.
//!
//! Before this, calling `hy.send_cap()` or passing invalid argument counts to
//! `hy.pipe` or `hy.new_pipe` passed `hyb check` with 0 diagnostics, failing only
//! downstream during `go build` (Row 13 BUILDABLE violation).

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
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn hy_capability_types_call_rejects() {
    rejects_e1002(r#"
fn main() {
    _ = hy.send_cap()
}
"#);
    rejects_e1002(r#"
fn main() {
    _ = hy.recv_cap()
}
"#);
    rejects_e1002(r#"
fn main() {
    _ = hy.channel()
}
"#);
    rejects_e1002(r#"
fn main() {
    _ = hy.send_cap(1)
}
"#);
}

#[test]
fn hy_pipe_arity_rejects() {
    // 2 arguments (expected at most 1)
    rejects_e1002(r#"
fn main() {
    _ = hy.pipe(1, 2)
}
"#);
    // 3 arguments (expected at most 1)
    rejects_e1002(r#"
fn main() {
    _ = hy.pipe(1, 2, 3)
}
"#);
}

#[test]
fn hy_new_pipe_arity_rejects() {
    // 2 arguments (expected at most 1)
    rejects_e1002(r#"
fn main() {
    _ = hy.new_pipe(1, 2)
}
"#);
    // 3 arguments (expected at most 1)
    rejects_e1002(r#"
fn main() {
    _ = hy.new_pipe(10, 20, 30)
}
"#);
}

#[test]
fn hy_spread_call_rejects() {
    rejects_e1002(r#"
fn main() {
    args := [10]
    _ = hy.pipe(args...)
}
"#);
    rejects_e1002(r#"
fn main() {
    args := [10]
    _ = hy.new_pipe(args...)
}
"#);
}

#[test]
fn hy_valid_calls_accept() {
    accepts(r#"
fn worker1(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}
fn worker2(tx hy.send_cap[int]) {
    tx.send(100)
    tx.close()
}
fn main() {
    tx1, rx1 := hy.pipe[int]()
    task worker1(move tx1)
    v1 := rx1.recv()
    _ = v1
    rx1.drain()

    tx2, rx2 := hy.pipe[int](10)
    task worker2(move tx2)
    v2 := rx2.recv()
    _ = v2
    rx2.drain()
}
"#);
}

#[test]
fn hy_shadowed_local_package_preserves_user_semantics() {
    accepts(r#"
struct custom_hy {
    val int
}
fn (h custom_hy) send_cap() int {
    return h.val
}
fn (h custom_hy) pipe(a int, b int) int {
    return a + b + h.val
}
fn main() {
    hy := custom_hy{val: 42}
    _ = hy.send_cap()
    _ = hy.pipe(1, 2)
}
"#);
}
