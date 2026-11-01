use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn closure_capture_reassign_and_shadow_negative_control_2315() {
    let r5 = check_file(&parse_file("t5.hy", "fn worker(m map[string]int, done chan<- bool) {\nm[\"w\"] = 1\ndone <- true\n}\nfn main() {\ndone := make(chan bool, 1)\nif true {\nm := map[string]int{\"k\": 0}\ntask worker(move m, done)\nf := fn() {\nm[\"c\"] = 2\n}\nf()\nm = map[string]int{\"f\": 3}\n_ = m\n}\n<-done\n}\n").unwrap());
    assert!(r5.verdict == Verdict::Reject && r5.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "{:?}", r5.diagnostics);

    let r6 = check_file(&parse_file("t6.hy", "fn worker(m map[string]int, done chan<- bool) {\nm[\"w\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"o\": 0}\ndone := make(chan bool, 1)\nif true {\nf := fn() {\nm[\"o\"] = 1\n}\nm := map[string]int{\"i\": 2}\ntask worker(move m, done)\nf()\n}\nm[\"m\"] = 3\n<-done\n}\n").unwrap());
    assert_eq!(r6.verdict, Verdict::Accept, "{:?}", r6.diagnostics);
}
