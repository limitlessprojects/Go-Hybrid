//! split_fork_join.rs — regression tests for the W-015 split spike:
//! `split s at mid into left, right { ... }` — lexical split-join via Go's
//! full slice expressions.
//!
//! Soundness pins:
//!   1. E3105 — the base must be a plain local slice with NO live sharers
//!      (may_share peers or alias parents reject the split).
//!   2. E3105 — a non-slice (or unknown) base fails closed.
//!   3. E3106 — the base is split-out for the whole block: any use of it
//!      inside the block rejects.
//!   4. The forks are fresh roots: moving BOTH into separate tasks in the
//!      same block accepts (that is the capability grant — mutual
//!      disjointness is Go's capacity-clip guarantee, race-verified in
//!      corpus 53).
//!   5. The base is usable again after the block.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn split_fork_join_moves_both_forks_into_tasks_accepts() {
    let r = check(
        "fn left(a []int, done chan int) {\n\
         \x20   for i in 0..len(a) {\n\
         \x20       a[i] = a[i] + 1\n\
         \x20   }\n\
         \x20   done <- 1\n\
         }\n\
         \n\
         fn right(b []int, done chan int) {\n\
         \x20   for i in 0..len(b) {\n\
         \x20       b[i] = b[i] * 2\n\
         \x20   }\n\
         \x20   done <- 1\n\
         }\n\
         \n\
         fn main() {\n\
         \x20   s := make([]int, 8)\n\
         \x20   mid := len(s) / 2\n\
         \x20   done := make(chan int, 2)\n\
         \x20   split s at mid into a, b {\n\
         \x20       task left(move a, done)\n\
         \x20       task right(move b, done)\n\
         \x20   }\n\
         \x20   <-done\n\
         \x20   <-done\n\
         \x20   fmt.println(s[0], s[7])\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}

#[test]
fn split_with_live_sharer_rejects_e3105() {
    let r = check(
        "fn main() {\n\
         \x20   s := make([]int, 8)\n\
         \x20   t := s\n\
         \x20   mid := len(s) / 2\n\
         \x20   split s at mid into a, b {\n\
         \x20       fmt.println(a[0], b[0])\n\
         \x20   }\n\
         \x20   fmt.println(t[0])\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert_eq!(codes(&r), vec!["HYB-E3105"]);
    assert!(
        r.diagnostics[0]
            .message
            .contains("live bindings share its storage: t"),
        "expected the sharer named in the diagnostic, got {:?}",
        r.diagnostics[0].message
    );
}

#[test]
fn split_use_of_base_inside_block_rejects_e3106() {
    let r = check(
        "fn main() {\n\
         \x20   s := make([]int, 8)\n\
         \x20   mid := len(s) / 2\n\
         \x20   split s at mid into a, b {\n\
         \x20       s[0] = 1\n\
         \x20       fmt.println(a[0], b[0])\n\
         \x20   }\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert_eq!(codes(&r), vec!["HYB-E3106"]);
}

#[test]
fn split_non_slice_base_fails_closed_e3105() {
    let r = check(
        "fn main() {\n\
         \x20   n := 5\n\
         \x20   split n at 2 into a, b {\n\
         \x20       fmt.println(a, b)\n\
         \x20   }\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert_eq!(codes(&r), vec!["HYB-E3105"]);
    assert!(
        r.diagnostics[0].message.contains("not a slice"),
        "expected the not-a-slice note, got {:?}",
        r.diagnostics[0].message
    );
}

#[test]
fn split_forks_die_at_block_edge_and_base_is_usable_after() {
    let r = check(
        "fn main() {\n\
         \x20   s := make([]int, 8)\n\
         \x20   mid := len(s) / 2\n\
         \x20   split s at mid into a, b {\n\
         \x20       a[0] = 1\n\
         \x20       b[0] = 2\n\
         \x20   }\n\
         \x20   s[0] = 9\n\
         \x20   fmt.println(s[0], s[7])\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}
