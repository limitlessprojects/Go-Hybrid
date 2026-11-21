//! #2356 — a `select` with two `default` arms passes `hyb check` and then
//! fails `go build` with `multiple defaults` (Row 13 BUILDABLE).
//!
//! Go allows at most one `default` clause per select statement. The parser
//! accepts more (the grammar has no such restriction) and the emitter prints
//! every arm, so the checker must reject the second and later defaults.
//!
//! The positive controls pin the cases that must stay accepted: one default
//! in any arm position, mixed with send and recv arms.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn e1002_count(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E1002")
        .count()
}

#[test]
fn two_defaults_reject_e1002() {
    let r = check(
        "fn main() {\n    select {\n        default: { fmt.println(0) }\n        default: { fmt.println(1) }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1, "one extra default: {:?}", r.diagnostics);
}

#[test]
fn three_defaults_reject_two_e1002() {
    let r = check(
        "fn main() {\n    select {\n        default: { fmt.println(0) }\n        default: { fmt.println(1) }\n        default: { fmt.println(2) }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(
        e1002_count(&r),
        2,
        "two extra defaults: {:?}",
        r.diagnostics
    );
}

#[test]
fn duplicate_default_after_recv_and_send_arms_rejects() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        v := <-ch: { fmt.println(v) }\n        default: { fmt.println(0) }\n        ch <- 1: { }\n        default: { fmt.println(1) }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(e1002_count(&r), 1, "{:?}", r.diagnostics);
}

#[test]
fn one_default_first_stays_accepted() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        default: { fmt.println(0) }\n        v := <-ch: { fmt.println(v) }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn one_default_last_stays_accepted() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        v := <-ch: { fmt.println(v) }\n        default: { fmt.println(0) }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn one_default_with_send_arm_stays_accepted() {
    let r = check(
        "fn main() {\n    ch := make(chan int, 1)\n    select {\n        ch <- 1: { }\n        default: { }\n    }\n}\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
