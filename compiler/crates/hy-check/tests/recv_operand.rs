//! recv_operand.rs — regression tests for issue #409: receive operands were
//! never type-checked, allowing `<-x` on non-channel values (primitive, map,
//! call results) to pass `hyb check` with 0 diagnostics and emit unbuildable Go.
//!
//! Symmetrically mirrors the send-side target validation (`check_send_target` / #77):
//!   1. A concrete non-channel receive operand is rejected as HYB-E3007 (rule O4).
//!   2. An unresolvable receive operand shape is fail-closed HYB-E3005 (rule O4.9).
//!   3. Covers bare receives (`<-x`), binding receives (`v := <-x`), select-arm
//!      receives (`case v := <-x:`), and closure bodies.
//!   4. Valid channel receives (channels, make(chan T), <-chan T parameters)
//!      stay cleanly accepted.

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

// ---------------------------------------------------------------------------
// Negative Cases: Concrete non-channel receives reject HYB-E3007
// ---------------------------------------------------------------------------

const R1_LITERAL_INT: &str = "\
fn main() {
    <-1
}
";

#[test]
fn r1_literal_int_recv_is_rejected_e3007() {
    let r = check(R1_LITERAL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for literal int receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R2_LOCAL_INT: &str = "\
fn main() {
    x := 5
    <-x
}
";

#[test]
fn r2_local_int_recv_is_rejected_e3007() {
    let r = check(R2_LOCAL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for local int receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R3_CALL_INT: &str = "\
fn f() int {
    return 5
}

fn main() {
    <-f()
}
";

#[test]
fn r3_call_int_recv_is_rejected_e3007() {
    let r = check(R3_CALL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for call int receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R4_MAP_RECV: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    <-m
}
";

#[test]
fn r4_map_recv_is_rejected_e3007() {
    let r = check(R4_MAP_RECV);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for map receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R5_BINDING_NON_CHAN: &str = "\
fn main() {
    x := 5
    v := <-x
}
";

#[test]
fn r5_binding_non_chan_recv_is_rejected_e3007() {
    let r = check(R5_BINDING_NON_CHAN);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for binding receive from int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R6_SELECT_ARM_NON_CHAN: &str = "\
fn main() {
    x := 5
    select {
    v := <-x:
        _ = v
    }
}
";

#[test]
fn r6_select_arm_non_chan_recv_is_rejected_e3007() {
    let r = check(R6_SELECT_ARM_NON_CHAN);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for select receive arm from int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R7_CLOSURE_BODY_NON_CHAN: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    body := fn() {
        <-m
    }
    task body()
}
";

#[test]
fn r7_closure_body_non_chan_recv_is_rejected_e3007() {
    let r = check(R7_CLOSURE_BODY_NON_CHAN);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 in closure body receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R8_VOID_CALL_RECV: &str = "\
fn noop() {
}

fn main() {
    <-noop()
}
";

#[test]
fn r8_void_call_recv_is_rejected_e3007() {
    let r = check(R8_VOID_CALL_RECV);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for unresolvable void call receive, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Positive Cases: Valid channel receives stay cleanly accepted
// ---------------------------------------------------------------------------

const P1_PLAIN_CHAN_RECV: &str = "\
fn main() {
    ch := make(chan int, 1)
    <-ch
}
";

#[test]
fn p1_plain_chan_recv_stays_accepted() {
    let r = check(P1_PLAIN_CHAN_RECV);
    assert!(
        codes(&r).is_empty(),
        "plain channel receive must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P2_BINDING_CHAN_RECV: &str = "\
fn main() {
    ch := make(chan int, 1)
    v := <-ch
    _ = v
}
";

#[test]
fn p2_binding_chan_recv_stays_accepted() {
    let r = check(P2_BINDING_CHAN_RECV);
    assert!(
        codes(&r).is_empty(),
        "binding channel receive must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P3_SELECT_ARM_CHAN_RECV: &str = "\
fn main() {
    ch := make(chan int, 1)
    select {
    v := <-ch:
        _ = v
    }
}
";

#[test]
fn p3_select_arm_chan_recv_stays_accepted() {
    let r = check(P3_SELECT_ARM_CHAN_RECV);
    assert!(
        codes(&r).is_empty(),
        "select-arm channel receive must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P4_DIRECTIONAL_CHAN_PARAM: &str = "\
fn worker(done <-chan bool) {
    <-done
}

fn main() {
    done := make(chan bool, 1)
    worker(done)
}
";

#[test]
fn p4_directional_chan_param_stays_accepted() {
    let r = check(P4_DIRECTIONAL_CHAN_PARAM);
    assert!(
        codes(&r).is_empty(),
        "directional channel parameter receive must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const R9_IF_COND_NON_CHAN_IN_CLOSURE: &str = "\
fn main() {
    x := 5
    body := fn() {
        if <-x {
            fmt.println(\"unreachable\")
        }
    }
    task body()
}
";

#[test]
fn r9_if_cond_non_chan_in_closure_rejected_e3007() {
    let r = check(R9_IF_COND_NON_CHAN_IN_CLOSURE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3007"),
        "expected HYB-E3007 for non-chan receive in if condition inside closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
