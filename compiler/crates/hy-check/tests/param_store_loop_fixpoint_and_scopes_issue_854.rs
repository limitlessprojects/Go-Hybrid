//! Regression tests for issue #854 and the loop-carried helper store gap.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).expect("parse should succeed"))
}
fn assert_e3003(src: &str, why: &str) {
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject, "{why}: expected Reject, got: {:?}", res.diagnostics);
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "{why}: expected HYB-E3003, got: {:?}", res.diagnostics);
}
fn assert_accepted(src: &str, why: &str) {
    let res = check(src);
    assert!(res.diagnostics.is_empty(), "{why}: expected no diagnostics, got: {:?}", res.diagnostics);
}

#[test]
fn loop_carried_store_in_range_loop_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\ntmp := other\nfor i in 0..2 { dst[0] = tmp\ntmp = src }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "iteration 1 stores other, iteration 2 stores src");
}

#[test]
fn loop_carried_store_in_while_loop_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\ntmp := other\ni := 0\nfor i < 2 { dst[0] = tmp\ntmp = src\ni = i + 1 }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "same chain through a for cond loop");
}

#[test]
fn loop_carried_store_from_fresh_literal_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int) {\ntmp := map[string]int{}\nfor i in 0..2 { dst[0] = tmp\ntmp = src }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "tmp starts as fresh literal");
}

#[test]
fn loop_carried_three_hop_chain_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\na := other\nb := other\nc := other\nfor i in 0..4 { dst[0] = c\nc = b\nb = a\na = src }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "needs three passes to reach fixpoint");
}

#[test]
fn loop_carried_through_nested_loops_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\na := other\nb := other\nfor i in 0..2 { for j in 0..2 { dst[0] = b\nb = a }\na = src }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "outer loop carries a = src into inner loop");
}

#[test]
fn predeclared_var_assigned_in_for_is_e3003_issue_854() {
    assert_e3003("fn link_via_for(dst []map[string]int, src map[string]int) {\nvar tmp map[string]int\nfor i in 0..1 { tmp = src }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink_via_for(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "issue #854: var tmp has no alias before loop");
}

#[test]
fn predeclared_var_assigned_in_match_arm_is_e3003_issue_854() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, o option[int]) {\nvar tmp map[string]int\nmatch o { some(v) => { tmp = src }\nnone => {} }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, some(1))\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "issue #854 case 1, match arm");
}

#[test]
fn predeclared_var_assigned_in_select_arm_is_e3003_issue_854() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, c chan int) {\nvar tmp map[string]int\nselect { <-c: { tmp = src }\ndefault: {} }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nc := make(chan int, 1)\nc <- 1\nlink(dst, m, c)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "issue #854 case 1, select arm");
}

#[test]
fn inner_block_shadow_keeps_outer_param_alias_is_e3003_issue_854() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int) {\n{ src := map[string]int{}\nsrc[\"x\"] = 1 }\ndst[0] = src\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "issue #854 case 2: block shadow does not erase outer");
}

#[test]
fn assignment_before_block_shadow_is_kept_is_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\ntmp := map[string]int{}\n{ tmp = src\ntmp := other\ntmp[\"x\"] = 1 }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "tmp = src precedes shadowing tmp := other");
}

#[test]
fn straight_line_store_control_still_e3003() {
    assert_e3003("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\ntmp := other\ntmp = src\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "control: no loop in helper");
}

#[test]
fn loop_carried_store_inline_in_caller_still_e3003() {
    assert_e3003("fn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\ntmp := o\nfor i in 0..2 { dst[0] = tmp\ntmp = m }\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "control: same loop inline in main");
}

#[test]
fn fresh_copy_per_iteration_is_accepted() {
    assert_accepted("fn fill(dst []map[string]int, src map[string]int) {\nfor i in 0..3 {\ntmp := map[string]int{}\nfor k, v in src { tmp[k] = v + i }\ndst[i] = tmp }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}, {}, {}}\nfill(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "neg_loop_fresh");
}

#[test]
fn loop_carried_fresh_values_are_accepted() {
    assert_accepted("fn link(dst []map[string]int, src map[string]int) {\ntmp := map[string]int{}\ni := 0\nfor i < 3 { dst[0] = tmp\ntmp = map[string]int{\"n\": len(src) + i}\ni = i + 1 }\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "neg_loop_carried_fresh");
}

#[test]
fn inner_block_shadow_does_not_leak_out_is_accepted() {
    assert_accepted("fn link(dst []map[string]int, src map[string]int) {\ntmp := map[string]int{}\n{ tmp := src\ntmp[\"seen\"] = 1 }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "neg_block_shadow");
}

#[test]
fn loop_body_shadow_does_not_leak_out_is_accepted() {
    assert_accepted("fn link(dst []map[string]int, src map[string]int, other map[string]int) {\ntmp := map[string]int{}\nfor i in 0..2 { tmp := src\ntmp[\"x\"] = i }\ndst[0] = tmp\nother[\"y\"] = 1\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\no := map[string]int{\"b\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, o)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "neg_loop_shadow_body");
}

#[test]
fn match_arm_binding_does_not_leak_out_is_accepted() {
    assert_accepted("fn link(dst []map[string]int, src map[string]int, o option[map[string]int]) {\ntmp := map[string]int{}\nmatch o { some(v) => { v[\"k\"] = len(src) }\nnone => {} }\ndst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) { s[0][\"race\"] = 1\ndone <- 1 }\nfn main() {\nm := map[string]int{\"a\": 1}\ndst := []map[string]int{{}}\nlink(dst, m, some(map[string]int{}))\ndone := make(chan int, 1)\ntask worker(move dst, done)\nm[\"a\"] = 2\n<-done\n}", "neg_match_binding");
}

#[test]
fn nested_loops_over_values_are_accepted() {
    assert_accepted("fn total(rows [][]int) int {\nacc := 0\nfor r in rows { for x in r { acc = acc + x } }\nreturn acc\n}\nfn sum_into(dst []int, rows [][]int) {\nfor i in 0..len(dst) { dst[i] = total(rows) + i }\n}\nfn worker(s []int, done chan int) { s[0] = 1\ndone <- 1 }\nfn main() {\nrows := [][]int{{1, 2}, {3}}\ndst := []int{0, 0}\nsum_into(dst, rows)\ndone := make(chan int, 1)\ntask worker(move dst, done)\nrows[0][0] = 9\n<-done\n}", "neg_nested_loops");
}
