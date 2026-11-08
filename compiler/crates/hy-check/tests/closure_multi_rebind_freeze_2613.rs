use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse");
    check_file(&f).diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.to_string()).collect()
}

#[test]
fn multival_and_comma_ok_rebinds_preserve_closure_capture_move_freeze_e3003() {
    for src in [
        "fn make_pair() (map[string]int, int) { return map[string]int{\"k\": 2}, 1 }\nfn worker(m map[string]int, done chan int) { m[\"k\"] = 1\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n m := map[string]int{\"init\": 0}\n var tag int = 0\n var f fn() = fn() {}\n if true { f = fn() { m[\"k\"] = 99 } }\n m, tag = make_pair()\n _ = tag\n task worker(move m, done)\n f()\n <-done\n}\n",
        "fn id_pair(src map[string]int) (map[string]int, int) { return src, 1 }\nfn worker(m map[string]int, done chan int) { m[\"k\"] = 1\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n m := map[string]int{\"init\": 0}\n var tag int = 0\n var f fn() = fn() {}\n if true { f = fn() { m[\"k\"] = 99 } }\n m2 := map[string]int{\"k\": 2}\n m, tag = id_pair(m2)\n _ = tag\n task worker(move m2, done)\n f()\n <-done\n}\n",
        "fn worker(b map[string]map[string]int, done chan int) { b[\"a\"] = map[string]int{\"k\": 1}\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n buckets := map[string]map[string]int{\"a\": map[string]int{\"k\": 2}}\n m := map[string]int{\"init\": 0}\n var ok bool = false\n var f fn() = fn() {}\n if true { f = fn() { m[\"k\"] = 99 } }\n m, ok = buckets[\"a\"]\n _ = ok\n task worker(move buckets, done)\n f()\n <-done\n}\n",
        "fn worker(m map[string]int, done chan int) { m[\"k\"] = 1\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n ch := make(chan map[string]int, 1)\n ch <- move map[string]int{\"k\": 2}\n m := map[string]int{\"init\": 0}\n var ok bool = false\n var f fn() = fn() {}\n if true { f = fn() { m[\"k\"] = 99 } }\n m, ok = <-ch\n _ = ok\n task worker(move m, done)\n f()\n <-done\n}\n",
        "fn worker(m map[string]int, done chan int) { m[\"k\"] = 1\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n m := map[string]int{\"init\": 0}\n var f fn() = fn() {}\n task worker(move m, done)\n if true {\n f = fn() { m[\"k\"] = 99 }\n m := map[string]int{\"shadow\": 1}\n _ = m\n }\n f()\n <-done\n}\n",
    ] {
        let codes = error_codes(src);
        assert!(codes.iter().any(|c| c == "HYB-E3003"), "expected HYB-E3003, got {:?}", codes);
    }
    let ok = parse_file("test.hy", "fn make_pair() (map[string]int, int) { return map[string]int{\"k\": 2}, 1 }\nfn worker(m map[string]int, done chan int) { m[\"k\"] = 1\n done <- 1 }\nfn main() {\n done := make(chan int, 1)\n m := map[string]int{\"init\": 0}\n var tag int = 0\n m, tag = make_pair()\n _ = tag\n task worker(move m, done)\n <-done\n}\n").expect("parse");
    assert_eq!(check_file(&ok).verdict, Verdict::Accept);
}