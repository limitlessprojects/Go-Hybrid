//! Regression test for #4769 — `hy-check(channel_liveness)`: the body of a
//! `for` was walked TWICE, so a linear SPSC capability moved exactly once
//! inside a loop that does not end in `break`/`return` was reported with
//! `HYB-E3001 "affine capability 'X' moved more than once"`.
//!
//! Root cause: `compiler/crates/hy-check/src/channel_liveness.rs`, the
//! `Stmt::For` arm walked `body.stmts` a second time inside
//! `if !has_unconditional_exit { ... }`. The second walk re-visits every
//! `Expr::Move` in the body; since the first walk had already marked the
//! capability as moved, `check_task_arg` saw `is_moved(cap) == true` and
//! emitted a false "moved more than once" diagnostic.
//!
//! The affected pattern is a linear send capability moved into a task
//! inside a plain `for` loop:
//!
//! ```hy
//! for i in 0..1 {
//!     task worker(move tx_alpha)
//! }
//! ```
//!
//! Each capability is moved exactly once, so the program must be ACCEPTED.
//! The same program with the loop replaced by an `if` was already accepted,
//! which is the contrast that isolates the loop arm as the differentiator.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> hy_check::CheckResult {
    let f = parse_file("test.hy", src).expect("parse failed");
    check_file(&f)
}

#[test]
fn test_4769_move_of_capability_inside_for_is_accepted() {
    // Two distinct linear send capabilities, each moved exactly once, both
    // inside the same `for` loop. Must be accepted: no capability is moved
    // twice, and none is used after its move.
    let src = r#"
fn worker_a(tx hy.send_cap[int]) {
    tx.send(1)
}

fn worker_b(tx hy.send_cap[int]) {
    tx.send(2)
}

fn main() {
    tx_alpha, rx_alpha := hy.pipe[int]()
    tx_beta, rx_beta := hy.pipe[int]()
    for i in 0..1 {
        task worker_a(move tx_alpha)
        task worker_b(move tx_beta)
        rx_alpha.drain()
        rx_beta.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "#4769: a capability moved once inside a `for` must not be reported as \
         moved more than once; diagnostics: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_4769_single_capability_moved_inside_for_is_accepted() {
    // Same shape, one capability — the minimal reproducer.
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
}

fn main() {
    tx, rx := hy.pipe[int]()
    for i in 0..1 {
        task worker(move tx)
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "#4769: single capability moved once inside a `for` must be accepted; \
         diagnostics: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_4769_loop_without_unconditional_exit_is_accepted() {
    // A `for` whose body does not end in `break`/`return` is exactly the
    // shape that triggered the second walk. A capability moved in the body
    // must still be accepted.
    //
    // Two details keep this test pinned to #4769 and nothing else:
    //  - the bound is `0..1`, so the body runs exactly once. A wider bound
    //    really would move `tx` on every iteration, and the loop back-edge
    //    replay in `freeze.rs` correctly reports that as a use-after-move
    //    (a different rule, not this one).
    //  - `rx` is drained AFTER the loop. Draining inside the body is a
    //    use-after-move of `rx` in the Tier-0 freeze pass (O5.2), also a
    //    different rule.
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(7)
}

fn main() {
    tx, rx := hy.pipe[int]()
    for i in 0..1 {
        task worker(move tx)
    }
    rx.drain()
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "#4769: `for` without an unconditional exit must not double-walk the \
         body; diagnostics: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_4769_control_if_is_accepted() {
    // The negative control: the identical program with `if` instead of `for`.
    // This was already accepted before the fix and must stay accepted — it
    // is the contrast that proves the `for` arm was the differentiator.
    let src = r#"
fn worker(tx hy.send_cap[int]) {
    tx.send(1)
}

fn main() {
    tx, rx := hy.pipe[int]()
    if true {
        task worker(move tx)
        rx.drain()
    }
}
"#;
    let res = check_src(src);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "#4769 control: the `if` form must be accepted; diagnostics: {:?}",
        res.diagnostics
    );
}
