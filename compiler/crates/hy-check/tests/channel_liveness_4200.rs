//! Conformance matrix test suite for SPSC channel capability & nursery lifecycle invariants (Milestone 1, #4200).
//!
//! Tests 20 canonical patterns across positive acceptance and negative rejections:
//! - P01: Basic happy path (pipe, move tx to worker, close tx in worker, drain rx in receiver) -> Accept
//! - P02: Undrained receiver at scope exit -> Reject (HYB-E4005)
//! - P03: Unclosed sender at scope exit -> Reject (HYB-E4004)
//! - P04: Affine use-after-move (send after move tx) -> Reject (HYB-E3001)
//! - P05: Reference capture across task boundary (&tx) -> Reject (HYB-E3002)
//! - P06: Multiple move of same capability -> Reject (HYB-E3001)
//! - P07: Multi-stage pipeline (stage1 -> stage2) -> Accept
//! - P08: Loop send with proper close and drain -> Accept
//! - P09: Early exit before tx.close() -> Reject (HYB-E4004)
//! - P10: Select receive with drain -> Accept
//! - P11: Unmoved cap capture across task boundary -> Reject (HYB-E3002)
//! - P12: Move rx to consumer task and close tx in main -> Accept
//! - P13: Drain rx via for-range loop -> Accept
//! - P14: Use after move in select statement -> Reject (HYB-E3001)
//! - P15: Explicit rx.drain() after worker completion -> Accept
//! - P16: Bidirectional pipes (two pairs) cleanly split and moved -> Accept
//! - P17: Nested nursery scopes with capability isolation -> Accept
//! - P18: Unclosed sender in nested block -> Reject (HYB-E4004)
//! - P19: Receiver use after move -> Reject (HYB-E3001)
//! - P20: Multiple independent pipes in single scope all properly terminated -> Accept

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse failed");
    check_file(&f)
}

#[test]
fn test_p01_happy_path_accept() {
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
        "P01 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p02_undrained_receiver_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    _ = rx
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "Expected HYB-E4005 for undrained rx: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p03_unclosed_sender_reject() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    _ = tx
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "Expected HYB-E4004 for unclosed tx: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p04_affine_use_after_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    tx.send(10)
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for use after move: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p05_reference_capture_reject() {
    let src = r#"
fn worker(tx *hy.send_cap[int]) {
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(&tx)
    tx.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "Expected HYB-E3002 for &tx capture: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p06_multiple_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    task worker(move tx)
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for double move: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p07_pipeline_multi_stage_accept() {
    let src = r#"
fn stage1(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn stage2(rx hy.recv_cap[int], tx hy.send_cap[int]) {
    v := rx.recv()
    tx.send(v)
    rx.drain()
    tx.close()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task stage1(move tx1)
    task stage2(move rx1, move tx2)
    res := rx2.recv()
    _ = res
    rx2.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P07 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p08_loop_send_accept() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    for i in 0..5 {
        tx.send(i)
    }
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    for v in rx {
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P08 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p09_unclosed_sender_in_block_reject() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    _ = tx
    if true {
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "Expected HYB-E4004: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p10_select_receive_accept() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(100)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    select {
        <-rx => {
            rx.drain()
        }
        default => {
            rx.drain()
        }
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P10 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p11_unmoved_capture_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(tx)
    tx.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "Expected HYB-E3002 for unmoved capture: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p12_move_rx_to_consumer_accept() {
    let src = r#"
fn consumer(rx hy.recv_cap[int]) {
    v := rx.recv()
    _ = v
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task consumer(move rx)
    tx.send(99)
    tx.close()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P12 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p13_for_range_drain_accept() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.send(1)
    tx.close()
    for v in rx {
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P13 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p14_use_after_move_in_select_reject() {
    let src = r#"
fn worker(rx hy.recv_cap[int]) {
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move rx)
    select {
        <-rx => {}
        default => {}
    }
    tx.close()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 in select: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p15_explicit_drain_accept() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(5)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P15 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p16_bidirectional_pipes_accept() {
    let src = r#"
fn node_a(tx hy.send_cap[int], rx hy.recv_cap[int]) {
    tx.send(1)
    tx.close()
    rx.drain()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task node_a(move tx1, move rx2)
    tx2.send(2)
    tx2.close()
    rx1.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P16 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p17_nested_scope_accept() {
    let src = r#"
fn main() {
    {
        tx, rx := hy.pipe[int]()
        tx.send(1)
        tx.close()
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P17 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p18_unclosed_sender_nested_block_reject() {
    let src = r#"
fn main() {
    {
        tx, rx := hy.pipe[int]()
        _ = tx
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "Expected HYB-E4004 in nested block: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p19_recv_use_after_move_reject() {
    let src = r#"
fn consumer(rx hy.recv_cap[int]) {
    rx.drain()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task consumer(move rx)
    v := rx.recv()
    _ = v
    tx.close()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for rx use after move: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_p20_multiple_pipes_accept() {
    let src = r#"
fn worker1(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn worker2(tx hy.send_cap[int]) {
    tx.send(2)
    tx.close()
}

fn main() {
    tx1, rx1 := hy.pipe[int]()
    tx2, rx2 := hy.pipe[int]()
    task worker1(move tx1)
    task worker2(move tx2)
    rx1.drain()
    rx2.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "P20 should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_match_arm_double_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    cmd := 1
    match cmd {
        1 => {
            task worker(move tx)
            task worker(move tx)
        }
    }
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for double move in match arm: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_defer_use_after_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task worker(move tx)
    defer tx.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for use of moved cap in defer: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_return_receive_moved_cap_reject() {
    let src = r#"
fn consumer(rx hy.recv_cap[int]) {
    rx.drain()
}

fn test_fn() int {
    tx, rx := hy.pipe[int]()
    task consumer(move rx)
    tx.close()
    return <-rx
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for return of moved rx: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_managed_block_double_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    managed("test", "concurrency") {
        task worker(move tx)
        task worker(move tx)
    }
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for double move inside managed block: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_4289_loop_backedge_double_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    for i in 0..10 {
        task worker(move tx)
    }
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for loop-backedge double move: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_4289_loop_unconditional_break_accept() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    for i in 0..10 {
        task worker(move tx)
        break
    }
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "Loop with unconditional break must accept single move: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_4289_closure_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    spawn_worker := fn() {
        task worker(move tx)
    }
    _ = spawn_worker
    task worker(move tx)
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for closure capability move conflict: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_4289_assign_pipe_double_move_reject() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    var tx hy.send_cap[int]
    var rx hy.recv_cap[int]
    tx, rx = hy.pipe[int]()
    task worker(move tx)
    task worker(move tx)
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "Expected HYB-E3001 for double move on assigned pipe: {:?}",
        res.diagnostics
    );
}
