//! Regression tests for issue #2308 and #854: loop-carried parameter stores
//! inside helper functions reach a fixpoint across loop back-edges, and
//! scope-aware alias merges preserve outer bindings.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).expect("parse should succeed"))
}

fn assert_e3003(src: &str, why: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "{why}: expected Reject, got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "{why}: expected HYB-E3003, got: {:?}",
        res.diagnostics
    );
}

fn assert_accepted(src: &str, why: &str) {
    let res = check(src);
    assert!(
        res.diagnostics.is_empty(),
        "{why}: expected no diagnostics, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn loop_carried_store_in_range_loop_is_e3003_issue_2308() {
    assert_e3003(
        "fn link(dst []map[string]int, src map[string]int, other map[string]int) {\n\
         tmp := other\n\
         for i in 0..2 { dst[0] = tmp\ntmp = src }\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         o := map[string]int{\"b\": 1}\n\
         dst := []map[string]int{{}}\n\
         link(dst, m, o)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "iteration 1 stores other, iteration 2 stores src (#2308)",
    );
}

#[test]
fn loop_carried_store_in_while_loop_is_e3003_issue_2308() {
    assert_e3003(
        "fn link(dst []map[string]int, src map[string]int, other map[string]int) {\n\
         tmp := other\n\
         i := 0\n\
         for i < 2 { dst[0] = tmp\ntmp = src\ni = i + 1 }\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         o := map[string]int{\"b\": 1}\n\
         dst := []map[string]int{{}}\n\
         link(dst, m, o)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "same chain through a while loop (#2308)",
    );
}

#[test]
fn loop_carried_store_from_fresh_literal_is_e3003() {
    assert_e3003(
        "fn link(dst []map[string]int, src map[string]int) {\n\
         tmp := map[string]int{}\n\
         for i in 0..2 { dst[0] = tmp\ntmp = src }\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         dst := []map[string]int{{}}\n\
         link(dst, m)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "tmp starts as fresh literal",
    );
}

#[test]
fn loop_carried_three_hop_chain_is_e3003() {
    assert_e3003(
        "fn link(dst []map[string]int, src map[string]int, other map[string]int) {\n\
         a := other\n\
         b := other\n\
         c := other\n\
         for i in 0..4 { dst[0] = c\nc = b\nb = a\na = src }\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         o := map[string]int{\"b\": 1}\n\
         dst := []map[string]int{{}}\n\
         link(dst, m, o)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "needs multiple passes to reach fixpoint",
    );
}

#[test]
fn predeclared_var_assigned_in_for_is_e3003_issue_854() {
    assert_e3003(
        "fn link_via_for(dst []map[string]int, src map[string]int) {\n\
         var tmp map[string]int\n\
         for i in 0..1 { tmp = src }\n\
         dst[0] = tmp\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         dst := []map[string]int{{}}\n\
         link_via_for(dst, m)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "issue #854: var tmp has no alias before loop",
    );
}

#[test]
fn inner_block_shadow_does_not_leak_out_is_accepted() {
    assert_accepted(
        "fn link(dst []map[string]int, src map[string]int) {\n\
         tmp := map[string]int{}\n\
         { tmp := src\ntmp[\"seen\"] = 1 }\n\
         dst[0] = tmp\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         dst := []map[string]int{{}}\n\
         link(dst, m)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "neg_block_shadow",
    );
}

#[test]
fn fresh_copy_per_iteration_is_accepted() {
    assert_accepted(
        "fn fill(dst []map[string]int, src map[string]int) {\n\
         for i in 0..3 {\n\
         tmp := map[string]int{}\n\
         for k, v in src { tmp[k] = v + i }\n\
         dst[i] = tmp }\n\
         }\n\
         fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\n\
         fn main() {\n\
         m := map[string]int{\"a\": 1}\n\
         dst := []map[string]int{{}, {}, {}}\n\
         fill(dst, m)\n\
         done := make(chan int, 1)\n\
         task worker(move dst, done)\n\
         m[\"a\"] = 2\n\
         <-done\n\
         }",
        "neg_loop_fresh",
    );
}
