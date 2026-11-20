//! Regression tests for #4322: `Stmt::If` / `Stmt::Match` / `Stmt::Select`
//! arms ran sequentially on shared scope state, so a `tx.close()` /
//! `rx.drain()` / `task w(move tx)` on ONE conditional branch marked the
//! pipe terminated for ALL paths and escaped HYB-E4004 / HYB-E4005 at
//! scope exit. The fix snapshots scope state, runs each arm from the
//! snapshot, and meets the results: `moved_caps` intersects (moved on
//! every path), `maybe_caps` unions (a one-arm move still flags a later
//! use on the join), and pipe typestates take the least-progressed arm
//! unless every arm discharged the obligation.

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

fn expect_accept(src: &str) {
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected accept, got: {:?}",
        res.diagnostics
    );
}

// ---------- rejection family (the #4322 / move-variant repros) ----------

#[test]
fn close_and_drain_in_if_only_rejects_e4004_e4005() {
    // #4322's repro verbatim: on the flag==false path neither close nor
    // drain runs, so both invariants must fire.
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
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected E4004 for if-only close: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected E4005 for if-only drain: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_in_if_only_rejects_e4004() {
    // The moved_caps half: `task w(move tx)` on one branch must not excuse
    // the close obligation on the untouched path.
    let src = r#"
fn w(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    flag := true
    if flag {
        task w(move tx)
    }
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E4004");
}

#[test]
fn nested_if_move_rejects_e4004() {
    let src = r#"
fn w(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    flag := true
    if flag {
        if flag {
            task w(move tx)
        }
    }
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E4004");
}

#[test]
fn match_move_arm_empty_wildcard_rejects_e4004() {
    let src = r#"
fn w(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    cmd := 1
    match cmd {
        1 => {
            task w(move tx)
        }
        _ => {
        }
    }
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E4004");
}

#[test]
fn select_drain_only_on_recv_arm_rejects_e4005() {
    // P10's evil twin: the `default` arm does NOT drain, so the drain on
    // the receive arm cannot excuse rx on every path.
    let src = r#"
fn w(tx hy.send_cap[int]) {
    tx.send(100)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    task w(move tx)
    select {
        <-rx => {
            rx.drain()
        }
        default => {
        }
    }
}
"#;
    expect_reject_code(src, "HYB-E4005");
}

#[test]
fn move_in_if_then_send_on_join_rejects_e3001() {
    // maybe-union: a one-arm move still makes the join-path send a
    // use-after-move on the taken path.
    let src = r#"
fn w(tx hy.send_cap[int]) {
    tx.send(1)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    flag := true
    if flag {
        task w(move tx)
    }
    tx.send(1)
    tx.close()
    rx.drain()
}
"#;
    expect_reject_code(src, "HYB-E3001");
}

// ---------- acceptance family (no false positives) ----------

#[test]
fn close_on_both_arms_accepts() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    flag := true
    if flag {
        tx.close()
    } else {
        tx.close()
    }
    rx.drain()
}
"#;
    expect_accept(src);
}

#[test]
fn close_in_if_then_close_on_join_accepts() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    flag := true
    if flag {
        tx.close()
    }
    tx.close()
    rx.drain()
}
"#;
    expect_accept(src);
}

#[test]
fn match_close_on_both_arms_accepts() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    cmd := 1
    match cmd {
        1 => {
            tx.close()
        }
        _ => {
            tx.close()
        }
    }
    rx.drain()
}
"#;
    expect_accept(src);
}

#[test]
fn select_no_default_single_arm_accepts() {
    // A select without `default` blocks forever on the uncovered path; a
    // path that never reaches scope exit carries no terminal obligation.
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    select {
        <-rx => {
            rx.drain()
        }
    }
    tx.close()
}
"#;
    expect_accept(src);
}

#[test]
fn literal_true_if_does_not_meet() {
    // `if true` runs exactly one path — the meet must not introduce a
    // phantom fallthrough arm. tx stays unclosed (E4004 fires) but rx is
    // drained on the only reachable path (no E4005).
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    if true {
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected E4004 for unclosed tx: {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "literal-true if must not flag drained rx: {:?}",
        res.diagnostics
    );
}
