use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn field_and_index_assign_propagate_move_non_owned_origin() {
    let r_o5_field_move = check("struct holder { m map[string]int }\nfn worker(h holder) { h.m[\"w\"] = 1 }\nfn bad(p map[string]int) {\nh := holder{m: map[string]int{\"init\": 0}}\nh.m = move p\ntask worker(move h)\n}");
    assert_eq!(r_o5_field_move.verdict, Verdict::Reject, "{:?}", r_o5_field_move.diagnostics);
    assert!(r_o5_field_move.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
    let r_o5_index_move = check("fn worker(xs []map[string]int) { xs[0][\"w\"] = 1 }\nfn bad(p map[string]int) {\nxs := []map[string]int{map[string]int{\"init\": 0}}\nxs[0] = move p\ntask worker(move xs)\n}");
    assert_eq!(r_o5_index_move.verdict, Verdict::Reject, "{:?}", r_o5_index_move.diagnostics);
    assert!(r_o5_index_move.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
    let r_o5_parallel_rebind = check("struct holder { m map[string]int }\nfn worker(h holder) { h.m[\"w\"] = 1 }\nfn bad(p map[string]int) {\nh := holder{m: map[string]int{\"init\": 0}}\np, h.m = map[string]int{\"fresh\": 1}, p\ntask worker(move h)\n}");
    assert_eq!(r_o5_parallel_rebind.verdict, Verdict::Reject, "{:?}", r_o5_parallel_rebind.diagnostics);
    assert!(r_o5_parallel_rebind.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
    let r_o5_multi_unpack = check("struct holder { m map[string]int }\nfn pair(x map[string]int) (int, map[string]int) { return 1, x }\nfn worker(h holder) { h.m[\"w\"] = 1 }\nfn bad(p map[string]int) {\nn := 0\nh := holder{m: map[string]int{\"init\": 0}}\nn, h.m = pair(move p)\ntask worker(move h)\n}");
    assert_eq!(r_o5_multi_unpack.verdict, Verdict::Reject, "{:?}", r_o5_multi_unpack.diagnostics);
    assert!(r_o5_multi_unpack.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
}
