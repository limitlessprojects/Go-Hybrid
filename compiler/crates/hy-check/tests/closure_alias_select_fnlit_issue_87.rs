use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn closure_local_alias_and_select_arm_fnlit_reject() {
    let r_alias = check("fn worker(m map[string]int, done chan<- bool) {\nm[\"k\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\ndone := make(chan bool, 1)\nf := fn() {\nm2 := m\ntask worker(move m2, done)\n}\nf()\nm[\"k\"] = 2\n<-done\n}");
    assert_eq!(r_alias.verdict, Verdict::Reject, "{:?}", r_alias.diagnostics);
    assert!(r_alias.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
    let r_select_fnlit = check("fn worker(m map[string]int, done chan<- bool) {\nm[\"k\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\ndone := make(chan bool, 1)\nch := make(chan int, 1)\nch <- 1\nselect {\n<-ch: {\nf := fn() {\ntask worker(m, done)\n}\nf()\n}\n}\nm[\"k\"] = 2\n<-done\n}");
    assert_eq!(r_select_fnlit.verdict, Verdict::Reject, "{:?}", r_select_fnlit.diagnostics);
    assert!(r_select_fnlit.diagnostics.iter().any(|d| d.code == "HYB-E3001"));
    let r_ascribed_and_assign = check("fn worker(m map[string]int, done chan<- bool) {\nm[\"k\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\ndone := make(chan bool, 1)\nf := fn() {\nvar m2 map[string]int = m\ntask worker(move m2, done)\nvar m3 map[string]int = map[string]int{\"k\": 0}\nm3 = m\ntask worker(move m3, done)\n}\nf()\nm[\"k\"] = 2\n<-done\n}");
    assert_eq!(r_ascribed_and_assign.verdict, Verdict::Reject, "{:?}", r_ascribed_and_assign.diagnostics);
    assert_eq!(r_ascribed_and_assign.diagnostics.iter().filter(|d| d.code == "HYB-E3004").count(), 2);
}
