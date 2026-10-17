use hy_check::check_file;
use hy_diag::Verdict;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_diag::CheckOutput {
    let file = parse_file("test.hy", src).expect("parse");
    check_file("test.hy", &file)
}

fn codes(out: &hy_diag::CheckOutput) -> Vec<String> {
    out.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn closure_direct_and_block_scoped_reassignment_to_captured_rejects_e3004() {
    // Issue #301 exact reproducer: `alias := ...; alias = m; task worker(move alias, done)`
    let r_direct = check(
        "fn worker(m map[string]int, done chan bool) {\n    for i in 0..10 {\n        m[\"k\"] = i\n    }\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 2)\n    spawn_it := fn() {\n        alias := map[string]int{\"k\": 1}\n        alias = m\n        task worker(move alias, done)\n    }\n    spawn_it()\n    spawn_it()\n    <-done\n    <-done\n}\n",
    );
    assert_eq!(r_direct.verdict, Verdict::Reject, "got {:?}", r_direct.diagnostics);
    assert!(codes(&r_direct).iter().any(|c| c == "HYB-E3004"));

    // Block-scoped reassignment inside closure (`if true { alias = m }`)
    let r_block = check(
        "fn worker(m map[string]int, done chan bool) {\n    m[\"k\"] = 1\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 1)\n    spawn_it := fn() {\n        alias := map[string]int{\"k\": 1}\n        if true {\n            alias = m\n        }\n        task worker(move alias, done)\n    }\n    spawn_it()\n    <-done\n}\n",
    );
    assert_eq!(r_block.verdict, Verdict::Reject, "got {:?}", r_block.diagnostics);
    assert!(codes(&r_block).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn closure_multi_value_define_from_captured_rejects_e3004() {
    // `alias, ok := m["k"]` inside a closure where `m` is a captured nested map
    let r_map_ok = check(
        "fn worker(m map[string]int, done chan bool) {\n    m[\"x\"] = 1\n    done <- true\n}\n\nfn main() {\n    m := map[string]map[string]int{\"k\": {\"x\": 0}}\n    done := make(chan bool, 1)\n    spawn_it := fn() {\n        alias, ok := m[\"k\"]\n        if ok {\n            task worker(move alias, done)\n        }\n    }\n    spawn_it()\n    <-done\n}\n",
    );
    assert_eq!(r_map_ok.verdict, Verdict::Reject, "got {:?}", r_map_ok.diagnostics);
    assert!(codes(&r_map_ok).iter().any(|c| c == "HYB-E3004"));

    // `alias, n := pair(m)` inside a closure where `pair` returns a reference-typed value
    let r_pair = check(
        "fn pair(m map[string]int) (map[string]int, int) {\n    return m, 1\n}\n\nfn worker(m map[string]int, done chan bool) {\n    m[\"x\"] = 1\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"x\": 0}\n    done := make(chan bool, 1)\n    spawn_it := fn() {\n        alias, _ := pair(m)\n        task worker(move alias, done)\n    }\n    spawn_it()\n    <-done\n}\n",
    );
    assert_eq!(r_pair.verdict, Verdict::Reject, "got {:?}", r_pair.diagnostics);
    assert!(codes(&r_pair).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn nested_closure_sees_parent_closure_reassignment_to_captured_e3004() {
    let r_nested = check(
        "fn worker(m map[string]int, done chan bool) {\n    m[\"k\"] = 1\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 1)\n    spawn_it := fn() {\n        alias := map[string]int{\"k\": 1}\n        if true {\n            inner := fn() {\n                task worker(move alias, done)\n            }\n            inner()\n        }\n        alias = m\n    }\n    spawn_it()\n    <-done\n}\n",
    );
    assert_eq!(r_nested.verdict, Verdict::Reject, "got {:?}", r_nested.diagnostics);
    assert!(codes(&r_nested).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn closure_fresh_local_not_reassigned_to_captured_accepts() {
    let r_ok = check(
        "fn worker(m map[string]int, done chan bool) {\n    m[\"k\"] = 1\n    done <- true\n}\n\nfn main() {\n    done := make(chan bool, 1)\n    spawn_it := fn() {\n        alias := map[string]int{\"k\": 1}\n        alias = map[string]int{\"k\": 2}\n        task worker(move alias, done)\n    }\n    spawn_it()\n    <-done\n}\n",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "got {:?}", r_ok.diagnostics);
}
