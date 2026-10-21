use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn composite_literal_move_of_non_owned_param_preserves_origin() {
    let r_define = check("struct holder { m map[string]int }\nfn worker(h holder) { h.m[\"k\"] = 1 }\nfn spawn_it(m map[string]int) {\nh := holder{m: move m}\ntask worker(move h)\n}");
    assert_eq!(r_define.verdict, Verdict::Reject, "{:?}", r_define.diagnostics);
    assert_eq!(r_define.diagnostics[0].code, "HYB-E3004");
    let r_assign = check("struct holder { m map[string]int }\nfn worker(h holder) { h.m[\"k\"] = 1 }\nfn spawn_it(m map[string]int) {\nh := holder{m: map[string]int{}}\nh = holder{m: move m}\ntask worker(move h)\n}");
    assert_eq!(r_assign.verdict, Verdict::Reject, "{:?}", r_assign.diagnostics);
    assert_eq!(r_assign.diagnostics[0].code, "HYB-E3004");
}
