use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn composite_closure_holder_overwrite_and_shadow_negative_controls_accept() {
    let hdr = "struct runner { run fn() }\nfn worker(x []int, done chan int) { x[0] = 1\n done <- 1 }\n";
    let ok_bodies = [
        "var r runner = runner{run: fn() {}}\n if true {\n s := []int{0}\n if true { r = runner{run: fn() { s[0] = 99 }} }\n r = runner{run: fn() {}}\n s = []int{1}\n task worker(move s, done)\n }\n r.run()",
        "s := []int{0}\n task worker(move s, done)\n if true {\n s := []int{1}\n r := runner{run: fn() { s[0] = 99 }}\n r.run()\n }",
        "s := []int{0}\n r := runner{run: fn() { s[0] = 99 }}\n if true {\n s := []int{1}\n s = []int{2}\n task worker(move s, done)\n }\n r.run()",
    ];
    for (idx, body) in ok_bodies.iter().enumerate() {
        let src = format!("{hdr}fn main() {{\n done := make(chan int, 1)\n {body}\n _ = <-done\n}}\n");
        let rep = check_file(&parse_file("test.hy", &src).expect("parse"));
        assert_eq!(rep.verdict, Verdict::Accept, "ok case {idx}: {:?}", rep.diagnostics);
    }
}
