use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("loop_backedge_replay_completeness.hy", src).expect("parse"))
}

#[test]
fn loop_backedge_replay_consumes_and_copy_may_share_reject_e3003() {
    let r_consume = check("import \"rust/flate2\"\nfn main() {\n  keep := flate2.new_writer(6)?\n  nxt := flate2.new_writer(6)?\n  for i in 0..2 {\n    _ = i\n    t := nxt\n    nxt = flate2.new_writer(6)?\n    out := t.finish()?\n    _ = out\n    keep.write([]byte(\"race\"))?\n    keep = nxt\n  }\n}");
    assert!(r_consume.diagnostics.iter().any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A7")));
    let r_copy = check("fn wm(x []map[string]int) {}\nfn main() {\n  keep := []map[string]int{map[string]int{\"k\": 0}}\n  nxt := []map[string]int{map[string]int{\"k\": 1}}\n  for i in 0..2 {\n    t := []map[string]int{map[string]int{\"k\": 2}}\n    _ = copy(t, nxt)\n    nxt = []map[string]int{map[string]int{\"k\": 3}}\n    task wm(move t)\n    keep[0][\"race\"] = i\n    keep = nxt\n  }\n}");
    assert!(r_copy.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}

#[test]
fn loop_backedge_replay_expr_if_match_and_return_moves_reject_e3003() {
    for src in [
        "fn sink(x map[string]int) {}\nfn main() {\n  keep := map[string]int{}\n  nxt := map[string]int{}\n  for i in 0..2 {\n    t := nxt\n    nxt = map[string]int{}\n    sink(move t)\n    keep[\"race\"] = i\n    keep = nxt\n  }\n}",
        "fn ok_map(x map[string]int) bool { return true }\nfn main() {\n  keep := map[string]int{}\n  nxt := map[string]int{}\n  for i in 0..2 {\n    t := nxt\n    nxt = map[string]int{}\n    if ok_map(move t) { keep[\"race\"] = i }\n    keep = nxt\n  }\n}",
        "fn tag_map(x map[string]int) int { return 1 }\nfn main() {\n  keep := map[string]int{}\n  nxt := map[string]int{}\n  for i in 0..2 {\n    t := nxt\n    nxt = map[string]int{}\n    match tag_map(move t) { _ => { keep[\"race\"] = i } }\n    keep = nxt\n  }\n}",
        "fn run() map[string]int {\n  keep := map[string]int{}\n  nxt := map[string]int{}\n  for i in 0..2 {\n    t := nxt\n    nxt = map[string]int{}\n    if i == 99 { return move t }\n    keep[\"race\"] = i\n    keep = nxt\n  }\n  return map[string]int{}\n}\nfn main() { _ = run() }",
    ] {
        assert!(check(src).diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }
}

#[test]
fn loop_backedge_replay_for_iterate_outer_and_inner_reject_e3003() {
    for src in [
        "fn main() {\n  xs := []map[string]int{map[string]int{\"k\": 1}, map[string]int{\"k\": 2}}\n  keep := map[string]int{\"k\": 0}\n  for _, v in xs {\n    t := keep\n    keep = map[string]int{\"k\": 3}\n    moved_t := move t\n    _ = moved_t\n    v[\"race\"] = 1\n    keep = v\n  }\n}",
        "fn main() {\n  xs := []map[string]int{map[string]int{\"k\": 1}}\n  keep := map[string]int{\"k\": 0}\n  for i in 0..2 {\n    t := keep\n    keep = map[string]int{\"k\": 3}\n    for _, v in xs {\n      moved_t := move t\n      _ = moved_t\n      v[\"race\"] = i\n      keep = v\n    }\n  }\n}",
    ] {
        assert!(check(src).diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }
}

#[test]
fn loop_backedge_replay_return_borrow_and_slot_loan_conflicts_reject() {
    let r_ret = check("struct node { v int }\nfn leak(keep *node, fallback *node) *node {\n  x := node{v: 1}\n  for i in 0..2 {\n    if i == 1 { return keep }\n    keep = &x\n  }\n  return fallback\n}\nfn main() {\n  m := map[string]node{\"k\": node{v: 0}}\n  s := &m[\"k\"]\n  _ = s\n}");
    assert!(r_ret.diagnostics.iter().any(|d| d.code == "HYB-E3104"));
    let r_del = check("struct node { v int }\nfn main() {\n  keep := map[string]node{\"k\": node{v: 1}}\n  nxt := map[string]node{\"k\": node{v: 2}}\n  s := &nxt[\"k\"]\n  for i in 0..2 {\n    delete(keep, \"k\")\n    s.v = i\n    keep = nxt\n  }\n}");
    assert!(r_del.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
}

#[test]
fn loop_backedge_replay_single_iteration_control_still_accepts() {
    let r_ok = check("fn sink(x map[string]int) {}\nfn main() {\n  keep := map[string]int{}\n  nxt := map[string]int{}\n  for i in 0..1 {\n    t := nxt\n    nxt = map[string]int{}\n    sink(move t)\n    keep[\"race\"] = i\n    keep = nxt\n  }\n}");
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
}
