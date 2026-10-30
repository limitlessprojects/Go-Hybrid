//! atomic_value_copy.rs — scalar sync/atomic values are copy-locked
//! (issue #1399).
//!
//! `TypeShape::Atomic` used to be sendable-by-type and const-copyable, so
//! every by-value copy of `atomic.bool` / `atomic.int32` / `atomic.int64` /
//! `atomic.uint32` / `atomic.uint64` / `atomic.uintptr` passed `hyb check
//! --deny-managed` with 0 diagnostics, then failed `go vet` (copylocks) and
//! raced under `go run -race` when a task held `&hits`. Decl copies and
//! whole-value assignments reject as HYB-E2011 (the statement-level guard);
//! boundary copies (task/spawn/send/move) reject as HYB-E3001/E3002 through
//! the sendability family. The controls pin the legal routes — `&hits`,
//! method calls, pointer copies — so the rule cannot over-reject.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert!(
        error_codes(&r).is_empty() && r.verdict == Verdict::Accept,
        "must accept, got {:?}\nsrc:\n{}",
        r.diagnostics,
        src
    );
}

// ---------------------------------------------------------------------------
// Rejects: statement-level value copies (HYB-E2011)
// ---------------------------------------------------------------------------

/// `other := hits` — a decl copy of a copy-locked value.
#[test]
fn rejects_decl_copy() {
    rejects_with(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    hits.add(1)
    other := hits
    _ = other
}
"#,
        "HYB-E2011",
    );
}

/// `hits = z` — a whole-value overwrite (the issue's race #2 shape).
#[test]
fn rejects_whole_value_assign() {
    rejects_with(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    var z atomic.int64
    hits = z
}
"#,
        "HYB-E2011",
    );
}

/// `hits += 1` — a non-atomic read-modify-write on the value itself.
#[test]
fn rejects_compound_assign() {
    rejects_with(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    hits += 1
}
"#,
        "HYB-E2011",
    );
}

// ---------------------------------------------------------------------------
// Rejects: boundary copies (the sendability family)
// ---------------------------------------------------------------------------

/// `task reader(hits)` — by-value task parameter (issue race #1).
#[test]
fn rejects_spawn_by_value() {
    rejects_with(
        r#"import "sync/atomic"

fn reader(a atomic.int64) {
    _ = a.load()
}

fn main() {
    var hits atomic.int64
    task reader(hits)
}
"#,
        "HYB-E3001",
    );
}

/// `ch <- hits` — by-value channel send.
#[test]
fn rejects_chan_send_by_value() {
    rejects_with(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    ch := make(chan atomic.int64, 1)
    ch <- hits
}
"#,
        "HYB-E3002",
    );
}

/// `move h` where the struct holds an atomic — the container copy moves the
/// copy-locked field by value too.
#[test]
fn rejects_move_struct_holding_atomic() {
    rejects_with(
        r#"import "sync/atomic"

struct holder {
    cnt atomic.int64
}

fn consume(h holder) {
    _ = h.cnt.load()
}

fn main() {
    var h holder
    task consume(move h)
}
"#,
        "HYB-E3001",
    );
}

// ---------------------------------------------------------------------------
// Controls: the legal routes (OQ6 pointer mediation)
// ---------------------------------------------------------------------------

/// Local ops through methods, no value copy.
#[test]
fn control_methods_accept() {
    accepts(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    hits.add(1)
    n := hits.load()
    _ = n
}
"#,
    );
}

/// `task worker(&hits)` — the mediated pointer route (corpus #17 shape).
#[test]
fn control_spawn_by_pointer_accept() {
    accepts(
        r#"import "sync/atomic"

fn worker(p *atomic.int64) {
    p.add(1)
}

fn main() {
    var hits atomic.int64
    task worker(&hits)
}
"#,
    );
}

/// A pointer binding copied by value is fine — only the atomic VALUE is
/// copy-locked.
#[test]
fn control_pointer_copy_accept() {
    accepts(
        r#"import "sync/atomic"

fn main() {
    var hits atomic.int64
    p := &hits
    q := p
    q.add(1)
}
"#,
    );
}
