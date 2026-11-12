use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn closure_capture_move_across_post_closure_block_shadow_2315() {
    let r3 = check_file(&parse_file("t3.hy", "fn worker(m map[string]int, done chan<- bool) {\nm[\"w\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\ndone := make(chan bool, 1)\ntask worker(move m, done)\nif true {\nf := fn() {\nm[\"c\"] = 2\n}\nf()\nm := map[string]int{\"f\": 3}\n_ = m\n}\n<-done\n}\n").unwrap());
    assert!(r3.verdict == Verdict::Reject && r3.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "{:?}", r3.diagnostics);

    let r4 = check_file(&parse_file("t4.hy", "fn worker(m map[string]int, done chan<- bool) {\nm[\"w\"] = 1\ndone <- true\n}\nfn main() {\ndone := make(chan bool, 1)\nif true {\nm := map[string]int{\"k\": 0}\nf := fn() {\nm[\"c\"] = 2\n}\ntask worker(move m, done)\nm := 1\n_ = m\nf()\n}\n<-done\n}\n").unwrap());
    assert!(r4.verdict == Verdict::Reject && r4.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "{:?}", r4.diagnostics);
}
