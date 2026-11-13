use hy_check::{check_file, Verdict};

fn check_e3001(b: &str, want: usize) {
    let s = format!("package main\nfn w(m map[string]int, c chan bool) {{\nm[\"x\"] = 1\nc <- true\n}}\nfn main() {{\n{b}\n}}\n");
    let r = check_file(&hy_syntax::parse_file("t.hy", &s).unwrap());
    let n = r
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E3003")
        .count();
    assert!(want > 0 && n >= want || want == 0 && n == 0 && r.verdict == Verdict::Accept);
}

#[test]
fn test_loop_backedge_shadow_and_lx14_739() {
    check_e3001("m := map[string]int{\"a\": 0}\nc := make(chan bool, 3)\ntask w(move m, c)\nif true { m := map[string]int{\"b\": 1}\nfor i in 0..2 { task w(move m, c) } }\nif true { m := map[string]int{\"c\": 2}\nfor i in 0..2 { task w(move m, c) } }", 2);
    check_e3001("c := make(chan bool, 2)\nf := fn() { m := map[string]int{\"a\": 0}\ntask w(move m, c)\nif true { m := map[string]int{\"b\": 1}\nfor i in 0..2 { task w(move m, c) } } }\nf()", 1);
    check_e3001("m := map[string]int{\"a\": 0}\nc := make(chan bool, 1)\nloop { task w(move m, c)\nfor i in 0..1 { continue loop }\nbreak }", 1);
    check_e3001("m := map[string]int{\"a\": 0}\nc := make(chan bool, 1)\nloop { task w(move m, c)\nloop { for i in 0..1 { break loop }\nreturn }\ncontinue }", 1);
    check_e3001("m := map[string]int{\"a\": 0}\nc := make(chan bool, 2)\ntask w(move m, c)\nif true { m := map[string]int{\"b\": 1}\ntask w(move m, c) }\nm2 := map[string]int{\"c\": 2}\nloop { task w(move m2, c)\nfor j in 0..1 { if j == 0 { continue } }\nbreak }", 0);
}
