//! Single-iteration range loop (`0..1`) back-edge check — value-parameter calls on the induction variable preserve single-iteration suppression.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_clean(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Accept, "unexpected {:?}: {:?} for:\n{src}", r.verdict, r.diagnostics);
}
const PRE: &str = "import \"fmt\"\nstruct svc { id int }\nstruct val_box { f fn(x int) }\nfn _use_fmt() { _ = fmt.sprint(0) }\nfn peek(x int) { _ = x }\nfn (s svc) peek(x int) { _ = x }\nfn worker(m map[string]int, done chan int) {\n    m[\"hits\"] = 1\n    done <- 1\n}";
#[test]
fn value_param_calls_with_induction_var_stay_accepted_in_single_iter_loop() {
    for call in [
        "peek(i)", "task peek(i)", "defer peek(i)", "f := fn(x int) { _ = x }\nf(i)", "s := svc{id: 1}\ns.peek(i)", "bx := val_box{f: peek}\nbx.f(i)", "_ = fmt.sprint(i)",
    ] { assert_clean(&format!("{PRE}\nfn main() {{\n    m := map[string]int{{\"hits\": 0}}\n    done := make(chan int, 1)\n    for i in 0..1 {{\n        {call}\n        task worker(move m, done)\n    }}\n    _ = <-done\n}}\n")); }
}
