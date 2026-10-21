use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn conditional_rebind_of_alias_preserves_outer_freeze() {
    let r_untaken = check("fn worker(m map[string]int) { m[\"k\"] = 1 }\nfn main() {\nm := map[string]int{\"k\": 0}\nm2 := m\nif false {\nm2 = map[string]int{\"k\": 1}\n}\ntask worker(move m)\nm2[\"k\"] = 2\n}");
    assert_eq!(r_untaken.verdict, Verdict::Reject, "{:?}", r_untaken.diagnostics);
    assert_eq!(r_untaken.diagnostics[0].code, "HYB-E3003");
    let r_branch_join = check("fn worker(m map[string]int) { m[\"k\"] = 1 }\nfn main() {\nm := map[string]int{\"k\": 0}\nm2 := map[string]int{\"k\": 1}\nif true {\nm2 = m\n} else {\nm2 = map[string]int{\"k\": 2}\n}\ntask worker(move m)\nm2[\"k\"] = 3\n}");
    assert_eq!(r_branch_join.verdict, Verdict::Reject, "{:?}", r_branch_join.diagnostics);
    assert_eq!(r_branch_join.diagnostics[0].code, "HYB-E3003");
}
#[test]
fn conditional_rebind_of_non_owned_param_preserves_origin() {
    let r = check("fn worker(m map[string]int) { m[\"k\"] = 1 }\nfn spawn_it(m map[string]int) {\nif false {\nm = map[string]int{\"k\": 1}\n}\ntask worker(move m)\n}");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(r.diagnostics[0].code, "HYB-E3004");
}
