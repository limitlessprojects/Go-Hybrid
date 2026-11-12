use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn closure_boundary_sendability_across_post_closure_block_shadow_2315() {
    let r1 = check_file(&parse_file("t1.hy", "fn worker(m map[string]int, done chan<- bool) {\nm[\"w\"] = 1\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\ndone := make(chan bool, 1)\nif true {\nf := fn() {\ntask worker(m, done)\n}\nf()\nm := 1\n_ = m\n}\nm[\"m\"] = 2\n<-done\n}\n").unwrap());
    assert!(r1.verdict == Verdict::Reject && r1.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{:?}", r1.diagnostics);

    let r2 = check_file(&parse_file("t2.hy", "fn worker(n int, done chan<- bool) {\ndone <- true\n}\nfn main() {\ndone := make(chan bool, 1)\nn := 42\nif true {\nf := fn() {\ntask worker(n, done)\n}\nf()\nn := 100\n_ = n\n}\n<-done\n}\n").unwrap());
    assert_eq!(r2.verdict, Verdict::Accept, "{:?}", r2.diagnostics);
}
