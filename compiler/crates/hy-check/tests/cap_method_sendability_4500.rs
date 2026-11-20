//! Regression tests for the capability-method boundary (issue fix):
//! `tx.send(v)` / `rx.recv()` on `hy.send_cap`/`hy.recv_cap` are channel
//! operations spelled as method calls — the payload argument must run the
//! same O4 sendability classification that statement `tx <- v` gets, the
//! method name must resolve inside the capability's finite surface, and
//! arity is fixed (send: 1, recv/close/drain: 0).
//!
//! Before the fix every row below was ACCEPTED with 0 diagnostics.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse failed");
    check_file(&f)
}

fn expect_reject_code(src: &str, code: &str) {
    let res = check_src(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn cap_send_map_payload_rejects_e3002() {
    // The report: a live map crosses the SPSC boundary unchecked while the
    // arrow-spelled twin (`tx <- m`) correctly rejects.
    let src = r#"
fn worker(rx hy.recv_cap[map[string]int]) {
    m := rx.recv()
    m["w"] = 1
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[map[string]int]()
    m := map[string]int{}
    task worker(move rx)
    tx.send(m)
    m["x"] = 2
    tx.close()
}
"#;
    expect_reject_code(src, "HYB-E3002");
}

#[test]
fn cap_send_zero_args_rejects_e1002() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send()
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_send_two_args_rejects_e1002() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send(1, 2)
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_recv_with_arg_rejects_e1002() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    v := rx.recv(9)
    _ = v
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_bogus_method_rejects_e1002() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.bogus(1)
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_recv_cap_send_rejects_e1002() {
    // `send` is not in `hy.recv_cap`'s surface — inverted direction.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    rx.send(1)
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_send_cap_recv_rejects_e1002() {
    // `recv` is not in `hy.send_cap`'s surface — inverted direction.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    v := tx.recv()
    _ = v
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E1002");
}

#[test]
fn cap_send_elem_shape_mismatch_rejects_e3011() {
    // `hy.pipe[int]` + `tx.send(map{...})`: the pipe's `[T]` is stamped on
    // the cap name, so `.send` gets the same element-vs-T check `tx <-`
    // gets through `chan_elem_shape` — a class-certain mismatch is
    // HYB-E3011 (issue #146's diagnostic, reused).
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send(map[string]int{})
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E3011");
}

#[test]
fn cap_send_primitive_into_map_pipe_rejects_e3011() {
    // The mirror image: an `int` payload into `hy.pipe[map[string]int]`.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[map[string]int]()
    tx.send(1)
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E3011");
}

#[test]
fn cap_arrow_send_elem_shape_mismatch_rejects_e3011() {
    // Arrow parity on the SAME element check: `chan_elem_shape` resolves
    // the stamped `[T]` for `tx <- v` too, so the two spellings cannot
    // diverge again.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx <- map[string]int{}
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E3011");
}

#[test]
fn cap_recv_result_carries_element_shape() {
    // `rx.recv()` on `hy.recv_cap[int]` must yield `int`'s shape — before
    // the `[T]` stamping it returned Unknown, and forwarding the received
    // value into another `send` mis-failed HYB-E3005 (O4.9). This is the
    // multi-stage-pipeline shape from channel_liveness P07.
    let src = r#"
fn fwd(v int) {
    _ = v
}

fn main() {
    tx, rx := hy.pipe[int]()
    v := rx.recv()
    fwd(v)
    tx.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "recv-forward must accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn cap_send_int_payload_still_accepts() {
    // Control: the canonical happy path stays green.
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    v := rx.recv()
    _ = v
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "happy path must still accept: {:?}",
        res.diagnostics
    );
}
