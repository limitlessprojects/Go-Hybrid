use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn field_and_index_assign_propagate_const_and_a3_loans() {
    let r_t22_field_move = check("struct holder { m map[string]int }\nconst shared_k = map[string]int{\"k\": 1}\nfn pair(x map[string]int) (int, map[string]int) { return 1, x }\nfn main() {\nh1 := holder{m: map[string]int{\"a\": 0}}\nh1.m = move shared_k\nh1.m[\"k\"] = 99\nn := 0\nh2 := holder{m: map[string]int{\"a\": 0}}\nn, h2.m = pair(move shared_k)\ndelete(h2.m, \"k\")\n}");
    assert_eq!(r_t22_field_move.verdict, Verdict::Reject, "{:?}", r_t22_field_move.diagnostics);
    let e3006_count = r_t22_field_move.diagnostics.iter().filter(|d| d.code == "HYB-E3006").count();
    assert_eq!(e3006_count, 2, "{:?}", r_t22_field_move.diagnostics);
    let r_a3_field_move = check("import \"fmt\"\nstruct svc { n int }\nstruct holder { m map[string]svc }\nfn main() {\nm := map[string]svc{\"k\": svc{n: 1}}\nslot := &m[\"k\"]\nh := holder{m: map[string]svc{}}\nh.m = move m\nh.m[\"other\"] = svc{n: 2}\nfmt.println(slot.n)\n}");
    assert_eq!(r_a3_field_move.verdict, Verdict::Reject, "{:?}", r_a3_field_move.diagnostics);
    assert!(r_a3_field_move.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
}
