//! Regression tests for Row 13 (BUILDABLE) soundness on `rust/flate2` crate calls:
//! The pinned `rust/flate2` boundary crate call surface must be arity-checked and
//! non-callable struct types (`flate2.writer`) must be rejected at `hyb check` time.
//!
//! Before this, calling `flate2.writer()` or passing invalid argument counts to
//! `flate2.new_writer`, `flate2.write`, or `flate2.finish` passed `hyb check` with 0
//! diagnostics, failing only downstream during `go build` (Row 13 BUILDABLE violation).

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
fn flate2_writer_type_call_rejects() {
    rejects_e1002(r#"
import "rust/flate2"
fn main() {
    _ = flate2.writer()
}
"#);
    rejects_e1002(r#"
import "flate2"
fn main() {
    _ = flate2.writer(1)
}
"#);
}

#[test]
fn flate2_new_writer_arity_rejects() {
    // 0 arguments (expected 1)
    rejects_e1002(r#"
import "rust/flate2"
fn main() {
    _ = flate2.new_writer()
}
"#);
    // 2 arguments (expected 1)
    rejects_e1002(r#"
import "rust/flate2"
import "bytes"
fn main() {
    var buf bytes.buffer
    _ = flate2.new_writer(&buf, 9)
}
"#);
}

#[test]
fn flate2_write_arity_rejects() {
    // 0 arguments (expected 2: w, data)
    rejects_e1002(r#"
import "rust/flate2"
fn main() {
    _ = flate2.write()
}
"#);
    // 1 argument (expected 2)
    rejects_e1002(r#"
import "rust/flate2"
import "bytes"
fn main() {
    var buf bytes.buffer
    w := flate2.new_writer(&buf)
    _ = flate2.write(w)
}
"#);
    // 3 arguments (expected 2)
    rejects_e1002(r#"
import "rust/flate2"
import "bytes"
fn main() {
    var buf bytes.buffer
    w := flate2.new_writer(&buf)
    _ = flate2.write(w, "hello", 123)
}
"#);
}

#[test]
fn flate2_finish_arity_rejects() {
    // 0 arguments (expected 1: w)
    rejects_e1002(r#"
import "rust/flate2"
fn main() {
    _ = flate2.finish()
}
"#);
    // 2 arguments (expected 1)
    rejects_e1002(r#"
import "rust/flate2"
import "bytes"
fn main() {
    var buf bytes.buffer
    w := flate2.new_writer(&buf)
    _ = flate2.finish(w, true)
}
"#);
}

#[test]
fn flate2_spread_call_rejects() {
    rejects_e1002(r#"
import "rust/flate2"
fn main() {
    args := ["hello"]
    _ = flate2.new_writer(args...)
}
"#);
}

#[test]
fn flate2_valid_calls_accept() {
    accepts(r#"
import "rust/flate2"
import "bytes"
fn main() {
    var buf bytes.buffer
    w := flate2.new_writer(&buf)
    _ = flate2.write(w, "hello")
    _ = flate2.finish(w)
}
"#);
}

#[test]
fn flate2_shadowed_local_package_preserves_user_semantics() {
    accepts(r#"
struct custom_flate {
    val int
}
fn (c custom_flate) writer() int {
    return c.val
}
fn (c custom_flate) new_writer(a int, b int) int {
    return a + b + c.val
}
fn main() {
    flate2 := custom_flate{val: 42}
    _ = flate2.writer()
    _ = flate2.new_writer(1, 2)
}
"#);
}
