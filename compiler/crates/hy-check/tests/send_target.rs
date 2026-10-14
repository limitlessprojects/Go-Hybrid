//! send_target.rs — regression tests for issue #77: send targets were
//! never classified, and the parser fused a next-line `<-` receive into
//! the previous expression statement.
//!
//! Two cooperating gaps turned ordinary snippets into programs that
//! `hyb check` accepted with 0 diagnostics and `hyb build --emit-go`
//! then emitted as INVALID Go (`noop() <- done` — "noop() (no value)
//! used as value"), breaking the accepted-should-emit-buildable-Go
//! contract:
//!
//! 1. Parser: `noop()` / newline / `<-done` fused into one Send. Fixed
//!    with Go-ASI-style newline sensitivity (parser.rs
//!    `same_line_as_prev`); parse-level pins live in hy-syntax
//!    parser tests (`newline_before_recv_does_not_fuse_into_send`).
//! 2. Checker: nothing validated that a send's target resolves to a
//!    chan shape. Fixed with `check_send_target` (sendability.rs): a
//!    concrete non-channel is HYB-E3007 (new registry code); an
//!    unresolvable shape is fail-closed HYB-E3005 (O4.9). Wired into
//!    statement sends, select-arm sends, and their closure-body twins.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const NEWLINE_RECV_IDIOM: &str = "\
fn noop() {
}

fn main() {
    done := make(chan bool, 1)
    noop()
    <-done
}
";

#[test]
fn newline_recv_idiom_is_two_statements_and_accepted() {
    // Repro A: before the parser fix this fused into `noop() <- done`
    // (invalid Go emission). Now: two statements — the call, then a
    // receive-expression statement — and the plain program is accepted.
    let r = check(NEWLINE_RECV_IDIOM);
    assert!(
        codes(&r).is_empty(),
        "the drain idiom must stay clean after de-fusion, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const DELIBERATE_NON_CHAN_SEND: &str = "\
fn noop() {
}

fn main() {
    noop() <- 1
}
";

#[test]
fn deliberate_void_call_send_is_rejected() {
    // Repro B: a void call as send target. `noop()`'s shape does not
    // resolve to a channel (no results -> Unknown), so the fail-closed
    // O4.9 path fires (HYB-E3005) — never OK again.
    let r = check(DELIBERATE_NON_CHAN_SEND);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005" || c == "HYB-E3007"),
        "expected E3005/E3007 for the void-call send target, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const CONCRETE_NON_CHAN_SEND: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    m <- 1
}
";

#[test]
fn concrete_non_channel_send_is_rejected_e3007() {
    // A RESOLVED non-channel (map) gets the concrete diagnostic
    // (HYB-E3007, registry: send-target-not-channel).
    let r = check(CONCRETE_NON_CHAN_SEND);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for the map send target, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const SELECT_ARM_NON_CHAN_SEND: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    select {
    m <- 1:
        fmt.println(\"x\")
    }
}
";

#[test]
fn select_arm_non_channel_send_is_rejected() {
    // The select-arm twin: arm heads go through the same target
    // classification.
    let r = check(SELECT_ARM_NON_CHAN_SEND);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007" || c == "HYB-E3005"),
        "expected E3007/E3005 for the select-arm map target, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const CLOSURE_BODY_NON_CHAN_SEND: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    body := fn() {
        m <- 1
    }
    task body()
}
";

#[test]
fn closure_body_non_channel_send_is_rejected() {
    // The closure-body twin (closures.rs check path).
    let r = check(CLOSURE_BODY_NON_CHAN_SEND);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007" || c == "HYB-E3005"),
        "expected E3007/E3005 in the closure body, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const PLAIN_SEND_ACCEPT: &str = "\
fn worker(done chan<- bool) {
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    worker(done)
    <-done
}
";

#[test]
fn plain_chan_send_stays_accepted() {
    // Precision control: real channel targets keep working — both the
    // send and the next-line receive drain.
    let r = check(PLAIN_SEND_ACCEPT);
    assert!(
        codes(&r).is_empty(),
        "a real channel send + drain must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
