use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn multival_define_and_assign_record_alias_and_may_share_e3003() {
    let r_def = check("fn worker(m map[string]int, done chan<- bool) {\nm[\"k\"] = 1\ndone <- true\n}\nfn pair(m map[string]int) (int, map[string]int) {\nreturn 1, m\n}\nfn main() {\nm := map[string]int{\"k\": 0}\nn, m2 := pair(m)\ndone := make(chan bool, 1)\ntask worker(move m, done)\nm2[\"k\"] = n\n<-done\n}");
    assert_eq!(r_def.verdict, Verdict::Reject, "{:?}", r_def.diagnostics);
    assert!(r_def.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    let r_ass = check("fn worker(m map[string]int, done chan<- bool) {\nm[\"k\"] = 1\ndone <- true\n}\nfn pair(m map[string]int) (int, map[string]int) {\nreturn 1, m\n}\nfn main() {\nm := map[string]int{\"k\": 0}\nvar n int\nvar m2 map[string]int\nn, m2 = pair(m)\ndone := make(chan bool, 1)\ntask worker(move m, done)\nm2[\"k\"] = n\n<-done\n}");
    assert_eq!(r_ass.verdict, Verdict::Reject, "{:?}", r_ass.diagnostics);
    assert!(r_ass.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    let r_comma_ok = check("fn worker(mm map[string]map[string]int, done chan<- bool) {\nmm[\"a\"][\"k\"] = 1\ndone <- true\n}\nfn main() {\ninner := map[string]int{\"k\": 0}\nmm := map[string]map[string]int{\"a\": inner}\nm2, ok := mm[\"a\"]\ndone := make(chan bool, 1)\ntask worker(move mm, done)\nif ok {\nm2[\"k\"] = 2\n}\n<-done\n}");
    assert_eq!(r_comma_ok.verdict, Verdict::Reject, "{:?}", r_comma_ok.diagnostics);
    assert!(r_comma_ok.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    let r_ok_only = check("fn worker(mm map[string]map[string]int, done chan<- bool) {\nmm[\"a\"][\"k\"] = 1\ndone <- true\n}\nfn main() {\ninner := map[string]int{\"k\": 0}\nmm := map[string]map[string]int{\"a\": inner}\n_, ok := mm[\"a\"]\ndone := make(chan bool, 1)\ntask worker(move mm, done)\nif ok {\n<-done\n}\n}");
    assert_eq!(r_ok_only.verdict, Verdict::Accept, "{:?}", r_ok_only.diagnostics);
}
