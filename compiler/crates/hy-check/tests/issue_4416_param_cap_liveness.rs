use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("repro.hy", src).expect("parse failed");
    check_file(&f)
}

fn assert_reject_code(src: &str, code: &str) {
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected Reject({code}): {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == code),
        "missing {code}: {:?}",
        res.diagnostics
    );
}

#[test]
fn unclosed_sender_and_undrained_receiver_params_rejected() {
    assert_reject_code(
        "fn worker(tx hy.send_cap[int]) { tx.send(42) }\n\
         fn main() { tx, rx := hy.pipe[int]()\n task worker(move tx)\n rx.drain() }",
        "HYB-E4004",
    );
    assert_reject_code(
        "fn receiver(rx hy.recv_cap[int]) { v := rx.recv()\n _ = v }\n\
         fn main() { tx, rx := hy.pipe[int]()\n tx.close()\n task receiver(move rx) }",
        "HYB-E4005",
    );
}

#[test]
fn double_move_and_use_after_move_param_caps_rejected_with_e3001() {
    let double_move = check_src(
        "fn sub(tx hy.send_cap[int]) { tx.close() }\n\
         fn forwarder(tx hy.send_cap[int]) { task sub(move tx)\n task sub(move tx) }\n\
         fn main() { tx, rx := hy.pipe[int]()\n task forwarder(move tx)\n rx.drain() }",
    );
    assert_eq!(double_move.verdict, Verdict::Reject);
    assert!(double_move
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3001"));
    assert!(!double_move
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E4004"));

    assert_reject_code(
        "fn sub(tx hy.send_cap[int]) { tx.close() }\n\
         fn forwarder(tx hy.send_cap[int]) { task sub(move tx)\n tx.send(10) }\n\
         fn main() { tx, rx := hy.pipe[int]()\n task forwarder(move tx)\n rx.drain() }",
        "HYB-E3001",
    );
}

#[test]
fn unmoved_ref_and_parenthesized_task_captures_rejected_with_e3002() {
    for src in [
        "fn sub(tx hy.send_cap[int]) { tx.close() }\n\
         fn forwarder(tx hy.send_cap[int]) { task sub(tx)\n tx.close() }",
        "fn sub(tx hy.send_cap[int]) { tx.close() }\n\
         fn forwarder(tx hy.send_cap[int]) { task sub((tx))\n tx.close() }",
        "fn sub_ptr(tx *hy.send_cap[int]) {}\n\
         fn forwarder(tx hy.send_cap[int]) { task sub_ptr((&tx))\n tx.close() }",
    ] {
        assert_reject_code(src, "HYB-E3002");
    }
}

#[test]
fn main_before_worker_and_lexical_non_cap_shadowing_accept() {
    let res = check_src(
        "fn main() int {\n\
           tx, rx := hy.pipe[int]()\n\
           task push_three(move tx)\n\
           task drain_all(move rx)\n\
           if true { tx := 41\n return tx + 1 }\n\
           return 0\n\
         }\n\
         fn push_three(tx hy.send_cap[int]) { for i in 0..3 { tx.send(i) }\n tx.close() }\n\
         fn drain_all(rx hy.recv_cap[int]) { for v in rx { _ = v } }",
    );
    assert_eq!(res.verdict, Verdict::Accept, "{:?}", res.diagnostics);
}

#[test]
fn aliased_closure_receiver_and_local_rebind_caps_enforced() {
    for (src, code) in [
        (
            "type int_tx = hy.send_cap[int]\nfn worker(tx int_tx) { tx.send(1) }",
            "HYB-E4004",
        ),
        (
            "fn main() { cb := fn(tx hy.send_cap[int]) { tx.send(1) }\n _ = cb }",
            "HYB-E4004",
        ),
        (
            "fn (tx hy.send_cap[int]) leak() { tx.send(1) }",
            "HYB-E4004",
        ),
        (
            "fn worker(tx hy.send_cap[int]) { t2 := move tx\n t2.send(42) }",
            "HYB-E4004",
        ),
        (
            "fn worker(tx hy.send_cap[int]) { t2 := tx\n tx.close()\n t2.send(99) }",
            "HYB-E3001",
        ),
    ] {
        assert_reject_code(src, code);
    }
}
