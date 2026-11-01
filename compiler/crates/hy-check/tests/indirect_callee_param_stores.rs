use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn rejects_indirect_fn_param_store_deep_chain_and_indirect_multiret() {
    for src in [
        "fn stash(m map[int][]int, xs []int) { m[0] = xs }\nfn w(m map[int][]int, d chan int) { m[0][0] = 1\n d <- 1 }\nfn main() { m := map[int][]int{}\n xs := []int{1}\n cb := stash\n cb(m, xs)\n d := make(chan int, 1)\n task w(move m, d)\n xs[0] = 2\n <-d }",
        "fn s6(m map[int][]int, xs []int) { m[0] = xs }\nfn s5(m map[int][]int, xs []int) { s6(m, xs) }\nfn s4(m map[int][]int, xs []int) { s5(m, xs) }\nfn s3(m map[int][]int, xs []int) { s4(m, xs) }\nfn s2(m map[int][]int, xs []int) { s3(m, xs) }\nfn s1(m map[int][]int, xs []int) { s2(m, xs) }\nfn w(m map[int][]int, d chan int) { m[0][0] = 1\n d <- 1 }\nfn main() { m := map[int][]int{}\n xs := []int{1}\n s1(m, xs)\n d := make(chan int, 1)\n task w(move m, d)\n xs[0] = 2\n <-d }",
        "struct bag { xs []int }\nfn dup() (bag, bag) { s := []int{1}\n return bag{xs: s}, bag{xs: s} }\nfn w(a bag, d chan int) { a.xs[0] = 1\n d <- 1 }\nfn main() { cb := dup\n a := bag{xs: []int{0}}\n b := bag{xs: []int{0}}\n a, b = cb()\n d := make(chan int, 1)\n task w(move a, d)\n b.xs[0] = 2\n <-d }",
    ] {
        let r = check_file(&parse_file("test.hy", src).expect("parse"));
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }
}
