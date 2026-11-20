//! Single-iteration range loop (`0..1`) back-edge check (`HYB-E3003`) — reject when the induction variable is auto-addressed into a pointer parameter.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3003(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "expected HYB-E3003, got {:?} for:\n{src}", r.diagnostics);
}
const PRE: &str = "import \"fmt\"\ntype int_ptr = *int\nstruct svc { id int }\nstruct mut_box { f fn(p *int) }\nfn rewind(p *int) { _, _ = fmt.sscan(\"-1\", p) }\nfn rewind_alias(p int_ptr) { _, _ = fmt.sscan(\"-1\", p) }\nfn rewind_gen[t](p *t) { _ = p }\nfn peek(x int) { _ = x }\nfn (s svc) rewind(p *int) { _, _ = fmt.sscan(\"-1\", p) }\nfn worker(m map[string]int, done chan int) {\n    m[\"hits\"] = 1\n    done <- 1\n}";
#[test]
fn auto_addressed_induction_var_in_single_iter_loop_rejects_backedge_move() {
    for (call, tail) in [
        ("rewind(i)", ""), ("rewind((i))", ""), ("rewind_alias(i)", ""), ("rewind_gen[int](i)", ""), ("task rewind(i)", ""), ("defer rewind(i)", ""),
        ("s := svc{id: 1}\ns.rewind(i)", ""), ("bx := mut_box{f: rewind}\nbx.f(i)", ""), ("f := fn(p *int) { _, _ = fmt.sscan(\"-1\", p) }\nf(i)", ""),
        ("(fn(p *int) { _, _ = fmt.sscan(\"-1\", p) })(i)", ""), ("peek := fn(p *int) { _, _ = fmt.sscan(\"-1\", p) }\npeek(i)", ""), ("rewind(i)", "rewind := fn(x int) { _ = x }\n_ = rewind"),
    ] { assert_e3003(&format!("{PRE}\nfn main() {{\n    if false {{\n        rewind := fn(x int) {{ _ = x }}\n        _ = rewind\n    }}\n    m := map[string]int{{\"hits\": 0}}\n    done := make(chan int, 2)\n    flipped := false\n    for i in 0..1 {{\n        if !flipped {{\n            flipped = true\n            {call}\n        }}\n        {tail}\n        task worker(move m, done)\n    }}\n    _ = <-done\n}}\n")); }
}
