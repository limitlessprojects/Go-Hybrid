use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn array_len_recv_and_div_zero_in_composite_and_local_types_reject() {
    let r1 = check_file(&parse_file("t.hy", "fn f(tx chan<- int) { _ = [][<-tx]int{} }\nfn main() {}").unwrap());
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E2008"), "{:?}", r1.diagnostics);
    for s in [
        "fn main() { var a [1 / 0]int\n _ = a }",
        "fn main() { _ = [][1 / 0]int{} }",
        "fn main() { _ = map[[1 / 0]int]int{} }",
        "fn main() { f := fn(x [1 / 0]int) { _ = x }\n _ = f }",
    ] {
        let r = check_file(&parse_file("t.hy", s).unwrap());
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E6001"), "{s}: {:?}", r.diagnostics);
    }
}
