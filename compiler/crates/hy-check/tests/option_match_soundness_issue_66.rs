use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
#[test]
fn option_some_unwrap_and_match_some_reject_and_freeze_aliases() {
    for (src, code) in [
        ("fn worker(m map[string]int, done chan<- bool) {\nfor i in 0..10000 { m[\"k\"] = i }\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\nvar opt option[map[string]int] = some(m)\nshared := 0\ndone := make(chan bool, 1)\nmatch opt {\nsome(shared): {\ntask worker(shared, done)\n}\nnone: {}\n}\nfor i in 0..10000 { m[\"k\"] = i }\n<-done\nfmt.println(shared)\n}", "HYB-E3001"),
        ("fn worker(opt option[map[string]int], done chan<- bool) {\nfor i in 0..10000 { opt.val[\"k\"] = i }\ndone <- true\n}\nfn main() {\nm := map[string]int{\"k\": 0}\nvar opt option[map[string]int] = some(m)\ndone := make(chan bool, 1)\ntask worker(move opt, done)\nfor i in 0..10000 { m[\"k\"] = i }\n<-done\n}", "HYB-E3003"),
        ("fn worker(m map[string]int) { m[\"k\"] = 1 }\nfn main() {\nm := map[string]int{\"k\": 0}\nvar opt option[map[string]int] = some(m)\nmatch opt {\nsome(x): { task worker(move x) }\nnone: {}\n}\nm[\"k\"] = 2\n}", "HYB-E3003"),
        ("fn worker(m map[int]int) { m[1] = 2 }\nfn main() {\nm := map[int]int{1: 1}\nopt := some(m)\nmatch opt {\nsome(opt): { task worker(move opt) }\nnone: {}\n}\n_ = m[1]\n}", "HYB-E3003"),
        ("fn sink(m map[int]int) { _ = m }\nfn helper(p option[map[int]int]) {\nmatch p {\nsome(b): { task sink(move b) }\nnone: {}\n}\n}", "HYB-E3004"),
        ("fn sink(opt option[map[int]int]) { _ = opt }\nfn main() {\nm := map[int]int{1: 1}\ntask sink(some(move m))\n_ = m[1]\n}", "HYB-E3003"),
        ("const m = map[int]int{1: 1}\nfn main() {\nf := fn() {\nmatch some(m) {\nsome(b): { b[1] = 2 }\nnone: {}\n}\n}\nf()\n}", "HYB-E3006"),
        ("fn mut_opt(opt option[map[int]int]) {\nmatch opt {\nsome(b): { b[2] = 20 }\nnone: {}\n}\n}\nfn main() {\nm := map[int]int{1: 10}\np := &m[1]\nmut_opt(some(m))\nfmt.println(p)\n}", "HYB-E3102"),
    ] {
        let r = check_file(&parse_file("test.hy", src).unwrap());
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == code), "expected {code}, got {:?}", r.diagnostics);
    }
}
