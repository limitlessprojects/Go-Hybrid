//! Regression tests for path-sensitive CFG branch/select join in channel_liveness.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse failed");
    check_file(&f)
}

#[test]
fn single_branch_if_close_drain_rejects_unclosed_else_path() {
    let src = r#"
fn maybe_close(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        tx.close()
        rx.drain()
    }
}

fn main() {
    maybe_close(false)
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 when else path skips tx.close(): {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected HYB-E4005 when else path skips rx.drain(): {:?}",
        res.diagnostics
    );
}

#[test]
fn if_else_missing_drain_in_else_rejects_e4005() {
    let src = r#"
fn check_branches(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        tx.close()
        rx.drain()
    } else {
        tx.close()
    }
}

fn main() {
    check_branches(false)
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected HYB-E4005 when else branch forgets rx.drain(): {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "both branches closed tx, unexpected HYB-E4004: {:?}",
        res.diagnostics
    );
}

#[test]
fn if_else_missing_close_in_then_rejects_e4004() {
    let src = r#"
fn check_branches(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        rx.drain()
    } else {
        tx.close()
        rx.drain()
    }
}

fn main() {
    check_branches(true)
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 when then branch forgets tx.close(): {:?}",
        res.diagnostics
    );
}

#[test]
fn if_else_both_branches_close_and_drain_accepts() {
    let src = r#"
fn check_branches(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        tx.send(1)
        tx.close()
        rx.drain()
    } else {
        tx.send(2)
        tx.close()
        rx.drain()
    }
}

fn main() {
    check_branches(true)
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "both branches properly terminated pipe: {:?}",
        res.diagnostics
    );
}

#[test]
fn if_else_one_closes_one_moves_accepts() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn run(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        tx.close()
    } else {
        task worker(move tx)
    }
    rx.drain()
}

fn main() {
    run(true)
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "then closes tx and else moves tx cleanly: {:?}",
        res.diagnostics
    );
}

#[test]
fn single_branch_if_move_rejects_unclosed_else_path() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.close()
}

fn run(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        task worker(move tx)
    }
    rx.drain()
}

fn main() {
    run(false)
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 when implicit else skips moving tx: {:?}",
        res.diagnostics
    );
}

#[test]
fn use_after_conditional_move_rejects_e3001() {
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.close()
}

fn run(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        task worker(move tx)
    } else {
        tx.close()
    }
    tx.send(1)
    rx.drain()
}

fn main() {
    run(true)
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for send after conditional move: {:?}",
        res.diagnostics
    );
}

#[test]
fn select_with_incomplete_arm_drain_rejects_e4005() {
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
        }
    }
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected HYB-E4005 when select default arm leaks rx: {:?}",
        res.diagnostics
    );
}

#[test]
fn conditional_close_followed_by_unconditional_close_accepts() {
    let src = r#"
fn run(flag bool) {
    tx, rx := hy.pipe[int]()
    if flag {
        tx.close()
    }
    tx.close()
    rx.drain()
}

fn main() {
    run(false)
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "unconditional close after if guarantees termination: {:?}",
        res.diagnostics
    );
}
