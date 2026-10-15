//! chan_direction.rs — regression tests for issue #89: channel DIRECTION
//! was never enforced. The parser carries `chan T` / `chan<- T` /
//! `<-chan T` (`TypeExpr::Chan { dir: ChanDir }`) and the emitter
//! faithfully reproduces the direction in the emitted Go signature, but
//! the checker accepted any send/receive on any channel binding — so
//! `fn worker(ch <-chan int) { ch <- 1 }` passed with 0 diagnostics and
//! then failed `go vet`/`go build` on the emitted program (the same
//! accepted-should-emit-buildable-Go contract break as #77).
//!
//! Fixed by `check_chan_dir` / `check_recv_expr` (sendability.rs):
//! HYB-E3008 at every send site (statement send, select send arm,
//! closure-body twins) and every receive site (statement-level `v := <-ch`
//! / `<-ch`, select recv arm, closure-body twins). The ascribed-locals gap
//! (`var sc chan<- int = c` — inference used to mask the narrowing
//! ascription in the binding's ty slot) is fixed in loans.rs process_local.

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

const RECV_FROM_SENDONLY_PARAM: &str = "\
fn worker(ch chan<- int) {
    <-ch
}

fn main() {
    c := make(chan int, 1)
    worker(c)
}
";

#[test]
fn recv_from_send_only_param_is_rejected() {
    // Repro 1: receiving from a `chan<- T` parameter.
    let r = check(RECV_FROM_SENDONLY_PARAM);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for receive from send-only param, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const SEND_TO_RECVONLY_PARAM: &str = "\
fn worker(ch <-chan int) {
    ch <- 1
}

fn main() {
    c := make(chan int, 1)
    worker(c)
}
";

#[test]
fn send_to_receive_only_param_is_rejected() {
    // Repro 2: sending to a `<-chan T` parameter.
    let r = check(SEND_TO_RECVONLY_PARAM);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for send to receive-only param, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const SELECT_RECV_FROM_SENDONLY: &str = "\
fn main() {
    c := make(chan int, 1)
    var sc chan<- int = c
    select {
    v := <-sc:
        fmt.println(v)
    }
}
";

#[test]
fn select_recv_arm_on_send_only_is_rejected() {
    // Repro 3a: select recv arm on a narrowed (`chan<-`) ascribed local.
    let r = check(SELECT_RECV_FROM_SENDONLY);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for select recv on send-only, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const SELECT_SEND_TO_RECVONLY: &str = "\
fn main() {
    c := make(chan int, 1)
    var rc <-chan int = c
    select {
    rc <- 1:
        fmt.println(\"x\")
    }
}
";

#[test]
fn select_send_arm_on_receive_only_is_rejected() {
    // Repro 3b: select send arm on a narrowed (`<-chan`) ascribed local.
    let r = check(SELECT_SEND_TO_RECVONLY);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for select send to receive-only, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const CLOSURE_SEND_TO_RECVONLY: &str = "\
fn main() {
    c := make(chan int, 1)
    var rc <-chan int = c
    body := fn() {
        rc <- 1
    }
    body()
}
";

#[test]
fn closure_body_send_to_receive_only_is_rejected() {
    // Repro 4: closure-body twin (closures.rs boundary pass).
    let r = check(CLOSURE_SEND_TO_RECVONLY);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 in the closure body, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const LOCAL_RECV_FROM_SENDONLY: &str = "\
fn main() {
    c := make(chan int, 1)
    var sc chan<- int = c
    v := <-sc
    fmt.println(v)
}
";

#[test]
fn local_recv_from_send_only_is_rejected() {
    // Statement-level `v := <-sc` receive (not select).
    let r = check(LOCAL_RECV_FROM_SENDONLY);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3008"),
        "expected HYB-E3008 for local recv from send-only, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const LEGAL_DIRECTION_USE: &str = "\
fn sendonly(ch chan<- int) {
    ch <- 1
}

fn recvonly(ch <-chan int) {
    <-ch
}

fn main() {
    c := make(chan int, 1)
    sendonly(c)
    recvonly(c)
}
";

#[test]
fn legal_direction_use_stays_accepted() {
    // Control: the corpus idiom — send-only params SEND, recv-only params
    // RECEIVE, caller passes a bidirectional channel (Go's implicit
    // bidir->unidir conversion at the call site). Must stay clean.
    let r = check(LEGAL_DIRECTION_USE);
    assert!(
        codes(&r).is_empty(),
        "legal direction usage must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const BIDIR_CONTROL: &str = "\
fn worker(ch chan int) {
    ch <- 1
}

fn main() {
    c := make(chan int, 1)
    worker(c)
    v := <-c
    fmt.println(v)
}
";

#[test]
fn bidirectional_channel_stays_accepted() {
    // Control: `chan T` send and receive (the common case).
    let r = check(BIDIR_CONTROL);
    assert!(
        codes(&r).is_empty(),
        "bidirectional send+recv must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
