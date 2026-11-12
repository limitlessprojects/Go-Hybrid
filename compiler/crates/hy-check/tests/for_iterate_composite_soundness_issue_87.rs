use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn for_iterate_composite_element_shares_with_collection_e3003() {
    let r = check("struct holder { m map[string]int }\nfn worker(hs []holder, done chan<- bool) {\nhs[0].m[\"k\"] = 1\ndone <- true\n}\nfn main() {\nhs := []holder{holder{m: map[string]int{\"k\": 0}}}\ndone := make(chan bool, 1)\nfor _, h in hs {\ntask worker(move hs, done)\nh.m[\"k\"] = 2\n}\n<-done\n}");
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
}
