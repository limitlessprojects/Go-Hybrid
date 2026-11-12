//! Regression tests for `compiler/crates/hy-check/src/freeze.rs`:
//! `check_loop_backedge_replay` must replay call sites (`process_call`),
//! `Stmt::Return` borrow/slot-loan escape checks (`process_return`),
//! and `Stmt::Expr` / `Stmt::If` / `Stmt::Match` move transfers across loop back-edges.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("freeze_loop_backedge_replay_calls.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn loop_backedge_replay_records_copy_may_share_from_reassigned_parameter() {
    let r = check(
        "fn worker(s [][]int, done chan<- bool) {\n\
           s[0][0] = 99\n\
           done <- true\n\
         }\n\
         fn leak(param [][]int) {\n\
           p := [][]int{[]int{1}}\n\
           for i in 0..2 {\n\
             dst := [][]int{[]int{1}}\n\
             copy(dst, p)\n\
             p = param\n\
             if i == 1 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               <-done\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 for copy(dst, p) after p = param on loop back-edge, got {:?}",
        r.diagnostics
    );
}

#[test]
fn loop_backedge_replay_records_callee_param_stores_from_reassigned_parameter() {
    let r = check(
        "struct holder {\n\
           m map[string]int\n\
         }\n\
         fn link(dst *holder, src *holder) {\n\
           dst.m = src.m\n\
         }\n\
         fn worker(h holder, done chan<- bool) {\n\
           h.m[\"k\"] = 99\n\
           done <- true\n\
         }\n\
         fn leak(param holder) {\n\
           p := holder{m: map[string]int{\"k\": 1}}\n\
           for i in 0..2 {\n\
             dst := holder{m: map[string]int{\"k\": 0}}\n\
             link(&dst, &p)\n\
             p = param\n\
             if i == 1 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               <-done\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 for link(&dst, &p) after p = param on loop back-edge, got {:?}",
        r.diagnostics
    );
}

#[test]
fn loop_backedge_replay_rejects_return_of_rebound_borrow_and_map_slot_loan() {
    let r_borrow = check(
        "struct acct {\n\
           bal int\n\
         }\n\
         fn escape_borrow(ext *acct) *acct {\n\
           local_a := acct{bal: 1}\n\
           p := ext\n\
           for i in 0..2 {\n\
             if i == 1 {\n\
               return p\n\
             }\n\
             p = &local_a\n\
           }\n\
           return ext\n\
         }",
    );
    assert_eq!(
        r_borrow.verdict,
        Verdict::Reject,
        "expected Reject, got {:?}",
        r_borrow.diagnostics
    );
    assert!(
        r_borrow.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 when returning p rebound to &local_a on loop back-edge, got {:?}",
        r_borrow.diagnostics
    );

    let r_slot = check(
        "struct acct {\n\
           bal int\n\
         }\n\
         fn escape_slot(ext *acct) *acct {\n\
           m := map[string]acct{\"a\": acct{bal: 1}}\n\
           p := ext\n\
           for i in 0..2 {\n\
             if i == 1 {\n\
               return p\n\
             }\n\
             p = &m[\"a\"]\n\
           }\n\
           return ext\n\
         }",
    );
    assert_eq!(
        r_slot.verdict,
        Verdict::Reject,
        "expected Reject, got {:?}",
        r_slot.diagnostics
    );
    assert!(
        r_slot.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 when returning p rebound to &m[\"a\"] on loop back-edge, got {:?}",
        r_slot.diagnostics
    );
}

#[test]
fn loop_backedge_replay_freezes_reassigned_source_after_copy_or_helper_store() {
    let r = check(
        "fn worker(s [][]int, done chan<- bool) {\n\
           s[0][0] = 99\n\
           done <- true\n\
         }\n\
         fn bad_copy_freeze() {\n\
           m2 := [][]int{[]int{2}}\n\
           p := [][]int{[]int{1}}\n\
           for i in 0..2 {\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, p)\n\
             p = m2\n\
             if i == 1 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               _ = m2[0][0]\n\
               <-done\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject, got {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when move dst freezes m2 via copy(dst, p) on loop back-edge, got {:?}",
        r.diagnostics
    );
}

#[test]
fn positive_control_loop_copy_and_return_without_shared_or_borrowed_backedge_accepts() {
    let r = check(
        "struct acct {\n\
           bal int\n\
         }\n\
         fn worker(s [][]int, done chan<- bool) {\n\
           s[0][0] = 99\n\
           done <- true\n\
         }\n\
         fn ok(ext *acct) *acct {\n\
           p := [][]int{[]int{1}}\n\
           q := ext\n\
           for i in 0..2 {\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, p)\n\
             p = [][]int{[]int{2}}\n\
             if i == 1 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               <-done\n\
               return q\n\
             }\n\
           }\n\
           return ext\n\
         }",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?}",
        r.diagnostics
    );
}

#[test]
fn multi_hop_loop_backedge_and_post_loop_state_propagation() {
    let r_2hop_freeze = check(
        "fn worker(s [][]int, done chan<- bool) {\n\
           s[0][0] = 99\n\
           done <- true\n\
         }\n\
         fn bad_copy_freeze_2hop() {\n\
           m2 := [][]int{[]int{2}}\n\
           p := [][]int{[]int{1}}\n\
           q := [][]int{[]int{0}}\n\
           for i in 0..3 {\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, q)\n\
             q = p\n\
             p = m2\n\
             if i == 2 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               _ = m2[0][0]\n\
               <-done\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r_2hop_freeze.verdict, Verdict::Reject, "{:?}", r_2hop_freeze.diagnostics);
    assert!(
        r_2hop_freeze.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on 2-hop loop-carried copy share, got {:?}",
        r_2hop_freeze.diagnostics
    );

    let r_2hop_param = check(
        "fn worker(s [][]int, done chan<- bool) {\n\
           s[0][0] = 99\n\
           done <- true\n\
         }\n\
         fn leak_param_2hop(param [][]int) {\n\
           p := [][]int{[]int{1}}\n\
           q := [][]int{[]int{0}}\n\
           for i in 0..3 {\n\
             dst := [][]int{[]int{0}}\n\
             copy(dst, q)\n\
             q = p\n\
             p = param\n\
             if i == 2 {\n\
               done := make(chan bool, 1)\n\
               task worker(move dst, done)\n\
               <-done\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r_2hop_param.verdict, Verdict::Reject, "{:?}", r_2hop_param.diagnostics);
    assert!(
        r_2hop_param.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on 2-hop loop-carried non-owned param share, got {:?}",
        r_2hop_param.diagnostics
    );

    let r_2hop_return = check(
        "struct acct {\n\
           bal int\n\
         }\n\
         fn escape_borrow_2hop(ext *acct) *acct {\n\
           local_a := acct{bal: 1}\n\
           p := ext\n\
           q := ext\n\
           for i in 0..3 {\n\
             if i == 2 {\n\
               return q\n\
             }\n\
             q = p\n\
             p = &local_a\n\
           }\n\
           return ext\n\
         }",
    );
    assert_eq!(r_2hop_return.verdict, Verdict::Reject, "{:?}", r_2hop_return.diagnostics);
    assert!(
        r_2hop_return.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
        "expected HYB-E3104 on 2-hop loop-carried borrow return, got {:?}",
        r_2hop_return.diagnostics
    );

    let r_post_loop_alias = check(
        "fn worker(s []int, done chan<- bool) {\n\
           s[0] = 99\n\
           done <- true\n\
         }\n\
         fn post_loop_race() {\n\
           m := []int{1, 2, 3}\n\
           t := []int{10, 20, 30}\n\
           u := []int{40, 50, 60}\n\
           for i in 0..2 {\n\
             u = t\n\
             t = m\n\
           }\n\
           done := make(chan bool, 1)\n\
           task worker(move u, done)\n\
           m[0] = 42\n\
           <-done\n\
         }",
    );
    assert_eq!(r_post_loop_alias.verdict, Verdict::Reject, "{:?}", r_post_loop_alias.diagnostics);
    assert!(
        r_post_loop_alias.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 when moving post-loop 2-hop alias u and writing m, got {:?}",
        r_post_loop_alias.diagnostics
    );

    let r_post_loop_nonowned = check(
        "fn worker(s []int, done chan<- bool) {\n\
           s[0] = 99\n\
           done <- true\n\
         }\n\
         fn post_loop_param(m []int) {\n\
           t := []int{10, 20, 30}\n\
           u := []int{40, 50, 60}\n\
           for i in 0..2 {\n\
             u = t\n\
             t = m\n\
           }\n\
           done := make(chan bool, 1)\n\
           task worker(move u, done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r_post_loop_nonowned.verdict, Verdict::Reject, "{:?}", r_post_loop_nonowned.diagnostics);
    assert!(
        r_post_loop_nonowned.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 when moving post-loop 2-hop parameter alias u, got {:?}",
        r_post_loop_nonowned.diagnostics
    );

    let r_ok = check(
        "fn worker(s []int, done chan<- bool) {\n\
           s[0] = 99\n\
           done <- true\n\
         }\n\
         fn ok_fresh_rebinds() {\n\
           t := []int{10, 20, 30}\n\
           u := []int{40, 50, 60}\n\
           for i in 0..3 {\n\
             u = t\n\
             t = []int{1, 2, 3}\n\
           }\n\
           done := make(chan bool, 1)\n\
           task worker(move u, done)\n\
           <-done\n\
         }",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "expected Accept, got {:?}", r_ok.diagnostics);
}
