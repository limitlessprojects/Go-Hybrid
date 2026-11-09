use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

#[test]
fn composite_closure_holders_across_scope_exit_and_enclosing_closures_e3003() {
    let hdr = "struct runner { run fn() }\nfn worker(x []int, done chan int) { x[0] = 1\n done <- 1 }\n";
    let bodies = [
        "var r runner = runner{run: fn() {}}\n if true {\n s := []int{0}\n if true { r = runner{run: fn() { s[0] = 99 }} }\n s = []int{1}\n task worker(move s, done)\n }\n r.run()",
        "outer := fn() {\n s := []int{0}\n r := runner{run: fn() { s[0] = 99 }}\n s = []int{1}\n task worker(move s, done)\n r.run()\n }\n outer()",
        "var outer fn() = fn() {}\n if true {\n s := []int{0}\n r := runner{run: fn() { s[0] = 99 }}\n outer = fn() { r.run() }\n s = []int{1}\n task worker(move s, done)\n }\n outer()",
        "s := []int{0}\n task worker(move s, done)\n if true {\n r := runner{run: fn() { s[0] = 99 }}\n s := []int{1}\n _ = s\n r.run()\n }",
    ];
    for (idx, body) in bodies.iter().enumerate() {
        let src = format!("{hdr}fn main() {{\n done := make(chan int, 1)\n {body}\n _ = <-done\n}}\n");
        let codes: Vec<_> = check_file(&parse_file("test.hy", &src).expect("parse"))
            .diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code).collect();
        assert!(codes.iter().any(|c| c == "HYB-E3003"), "scope case {idx} got {codes:?}");
    }
}
