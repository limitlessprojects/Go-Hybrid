//! Regression tests for SPSC linear channel capability blank-move discard,
//! local move-rebinding, and non-move local aliasing (#4200, #4558).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse failed");
    check_file(&f)
}

#[test]
fn blank_move_discard_rejects_unclosed_tx_and_undrained_rx() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    _ = move tx
    _ = move rx
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 for blank-discarded unclosed tx: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected HYB-E4005 for blank-discarded undrained rx: {:?}",
        res.diagnostics
    );
}

#[test]
fn blank_move_discard_after_close_and_drain_accepts() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx.close()
    rx.drain()
    _ = move tx
    _ = move rx
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "already closed/drained caps discarded via blank move should accept: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_rebind_define_leaked_tx_rejects() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := move tx
    _ = tx2
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 for leaked rebound tx2: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_rebind_define_leaked_rx_rejects() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    rx2 := move rx
    tx.close()
    _ = rx2
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4005"),
        "expected HYB-E4005 for leaked rebound rx2: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_rebind_assign_leaked_tx_rejects() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    var tx2 hy.send_cap[int]
    tx2 = move tx
    _ = tx2
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E4004"),
        "expected HYB-E4004 for leaked assigned tx2: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_rebind_use_after_move_on_original_rejects() {
    let src = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := move tx
    tx.send(1)
    tx2.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for use of original tx after move-rebind: {:?}",
        res.diagnostics
    );
}

#[test]
fn move_rebind_ref_capture_across_task_rejects() {
    let src = r#"
fn worker(tx *hy.send_cap[int]) {
    _ = tx
}

fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := move tx
    task worker(&tx2)
    tx2.close()
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "expected HYB-E3002 for reference capture of rebound tx2 across task boundary: {:?}",
        res.diagnostics
    );
}

#[test]
fn non_move_local_alias_rejects() {
    let define_alias = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := tx
    tx.close()
    rx.drain()
    _ = tx2
}
"#;
    let res1 = check_src(define_alias);
    assert_eq!(res1.verdict, Verdict::Reject);
    assert!(
        res1.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "expected HYB-E3002 for non-move local define alias: {:?}",
        res1.diagnostics
    );

    let assign_alias = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    var rx2 hy.recv_cap[int]
    rx2 = rx
    tx.close()
    rx.drain()
    _ = rx2
}
"#;
    let res2 = check_src(assign_alias);
    assert_eq!(res2.verdict, Verdict::Reject);
    assert!(
        res2.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "expected HYB-E3002 for non-move local assign alias: {:?}",
        res2.diagnostics
    );
}

#[test]
fn move_rebind_properly_terminated_or_transferred_accepts() {
    let direct_close_drain = r#"
fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := move tx
    rx2 := move rx
    tx2.send(1)
    tx2.close()
    rx2.drain()
}
"#;
    let res1 = check_src(direct_close_drain);
    assert_eq!(
        res1.verdict,
        Verdict::Accept,
        "rebound tx2/rx2 closed and drained should accept: {:?}",
        res1.diagnostics
    );

    let task_transfer = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(42)
    tx.close()
}

fn main() {
    tx, rx := hy.pipe[int]()
    tx2 := move tx
    task worker(move tx2)
    rx.drain()
}
"#;
    let res2 = check_src(task_transfer);
    assert_eq!(
        res2.verdict,
        Verdict::Accept,
        "rebound tx2 transferred to task should accept: {:?}",
        res2.diagnostics
    );
}

#[test]
fn blank_assign_without_move_still_reports_terminal_leak() {
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
        "expected HYB-E4004 for unclosed tx silenced via `_ = tx`: {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E3002"),
        "did not expect HYB-E3002 for blank silencer `_ = tx`: {:?}",
        res.diagnostics
    );
}
