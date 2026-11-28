//! Regression tests (reject) for #2419: loop back-edge replay of A7 consuming
//! calls and A1b move transfers across statement forms (`HYB-E3003`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn assert_e3003(src: &str, expected_rule: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some(expected_rule)),
        "expected HYB-E3003 ({expected_rule}), got {:?} for:\n{src}",
        res.diagnostics
    );
}

#[test]
fn loop_backedge_replay_rejects_a7_ownhandle_use_after_consume() {
    assert_e3003(
        "import \"rust/flate2\"\nfn compress() error {\n    keep := flate2.new_writer(6)?\n    nxt := flate2.new_writer(6)?\n    for i in 0..2 {\n        t := nxt\n        nxt = flate2.new_writer(6)?\n        _ = t.finish()?\n        _ = keep.write(\"chunk\")?\n        keep = nxt\n    }\n    return nil\n}\nfn main() {}\n",
        "A7",
    );
}

#[test]
fn loop_backedge_replay_rejects_expr_and_if_move_transfers() {
    assert_e3003(
        "fn sink(m map[string]int) {}\nfn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        sink(move t)\n        keep[\"race\"] = i\n        keep = nxt\n    }\n}\n",
        "A1b",
    );
    assert_e3003(
        "fn pred(m map[string]int) bool { return len(m) > 0 }\nfn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        if pred(move t) {\n            keep[\"race\"] = i\n        }\n        keep = nxt\n    }\n}\n",
        "A1b",
    );
}

#[test]
fn loop_backedge_replay_rejects_match_nested_for_and_return_move_transfers() {
    assert_e3003(
        "fn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        match some(move t) {\n            some(_): { keep[\"race\"] = i }\n            none: {}\n        }\n        keep = nxt\n    }\n}\n",
        "A1b",
    );
    assert_e3003(
        "fn keys(m map[string]int) []int { return []int{len(m)} }\nfn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        for x in keys(move t) {\n            keep[\"race\"] = x\n        }\n        keep = nxt\n    }\n}\n",
        "A1b",
    );
    assert_e3003(
        "fn run(stop bool) map[string]int {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        if stop {\n            return move t\n        }\n        keep[\"race\"] = i\n        keep = nxt\n    }\n    return keep\n}\nfn main() {}\n",
        "A1b",
    );
}
