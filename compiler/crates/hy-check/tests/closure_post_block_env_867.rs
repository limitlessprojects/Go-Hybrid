#[test]
fn closure_in_block_preserves_post_block_move_and_origin_867() {
    let src = "fn w(m map[string]int, d chan bool) {\nm[\"k\"] = 1\nd <- true\n}\nfn leak(p map[string]int, d chan bool) {\nvar m map[string]int\nvar f fn()\nif true {\nf = fn() {\ntask w(move m, d)\n}\n}\nm = p\nf()\n}\nfn race() {\nd := make(chan bool, 1)\nm := map[string]int{\"k\": 0}\nvar f fn()\nif true {\nf = fn() {\nm[\"k\"] = 2\n}\n}\ntask w(move m, d)\nf()\n<-d\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", src).unwrap());
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003") && r.diagnostics.iter().any(|d| d.code == "HYB-E3004"), "{:?}", r.diagnostics);
}
