use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn err_codes(src: &str) -> Vec<String> {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
        .diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code).collect()
}

const PRELUDE: &str = "struct box { val int }\ntype ptr_map = map[*box]int\nfn worker(p *box, done chan int) { p.val = 1\n done <- 1 }\nfn worker_a(a [1]*box, done chan int) { a[0].val = 1\n done <- 1 }\nfn worker_i(x int, done chan int) { done <- x }\n";

#[test]
fn single_var_map_key_alias_move_rejects_across_shapes_and_iterables() {
    let repro = format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  b := &box{{val: 0}}\n  m := map[*box]int{{b: 1}}\n  for k in m {{\n    alias := k\n    task worker(move alias, done)\n    k.val = 99\n  }}\n  _ = <-done\n}}\n");
    let c = err_codes(&repro);
    assert!(c.contains(&"HYB-E3003".to_string()) && c.contains(&"HYB-E3004".to_string()), "repro codes: {c:?}");

    let cases = [
        format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  m := map[[1]*box]int{{[1]*box{{&box{{val: 0}}}}: 1}}\n  for k in m {{\n    alias := k\n    task worker_a(move alias, done)\n    k[0].val = 99\n  }}\n  _ = <-done\n}}\n"),
        format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  var m ptr_map = map[*box]int{{&box{{val: 0}}: 1}}\n  for k in m {{\n    alias := k\n    task worker(move alias, done)\n    k.val = 99\n  }}\n  _ = <-done\n}}\n"),
        "import \"rust/serde_json\"\nstruct holder { doc serde_json.value }\nstruct wrap { m map[holder]int }\nfn worker_h(h holder, done chan int) { _ = h\n done <- 1 }\nfn run(w wrap) {\n  done := make(chan int, 1)\n  for k in w.m {\n    alias := k\n    task worker_h(move alias, done)\n    _ = k.doc\n  }\n  _ = <-done\n}\n".to_string(),
        format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  ms := []map[*box]int{{map[*box]int{{&box{{val: 0}}: 1}}}}\n  for k in ms[0] {{\n    alias := k\n    task worker(move alias, done)\n    k.val = 99\n  }}\n  _ = <-done\n}}\n"),
        format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  ch := make(chan map[*box]int, 1)\n  ch <- map[*box]int{{&box{{val: 0}}: 1}}\n  for k in <-ch {{\n    alias := k\n    task worker(move alias, done)\n    k.val = 99\n  }}\n  _ = <-done\n}}\n"),
        format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  b := &box{{val: 0}}\n  m := map[*box]int{{b: 1}}\n  var saved *box = b\n  var hit = false\n  for k in m {{\n    if hit {{\n      task worker(move saved, done)\n      k.val = 99\n    }}\n    saved = k\n    hit = true\n  }}\n  _ = <-done\n}}\n"),
    ];
    for (idx, src) in cases.iter().enumerate() {
        let codes = err_codes(src);
        assert!(codes.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3004"), "case {idx} accepted: {codes:?}");
    }
}

#[test]
fn primitive_key_and_two_var_map_range_negative_controls() {
    let copy_key = format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  m := map[int]*box{{1: &box{{val: 7}}}}\n  for k in m {{\n    alias := k\n    task worker_i(alias, done)\n    _ = k + 1\n  }}\n  _ = <-done\n}}\n");
    let copy_codes = err_codes(&copy_key);
    assert!(copy_codes.is_empty(), "Copy int key must be accepted, got: {copy_codes:?}");

    let two_var_val = format!("{PRELUDE}fn main() {{\n  done := make(chan int, 1)\n  m := map[int]*box{{1: &box{{val: 7}}}}\n  for k, v in m {{\n    alias := v\n    task worker(move alias, done)\n    v.val = k\n  }}\n  _ = <-done\n}}\n");
    assert!(err_codes(&two_var_val).contains(&"HYB-E3003".to_string()), "2-var map value must reject");
}
