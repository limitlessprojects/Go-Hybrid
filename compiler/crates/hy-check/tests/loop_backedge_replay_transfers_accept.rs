//! Regression tests (accept controls) for #2419: single-iteration, trailing-break,
//! and fresh per-iteration handle/map loops remain accepted.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_accept(src: &str) {
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", res.diagnostics);
}

#[test]
fn single_iteration_and_trailing_break_loops_accept() {
    assert_accept(
        "fn sink(m map[string]int) {}\nfn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..1 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        sink(move t)\n        keep[\"ok\"] = i\n        keep = nxt\n    }\n}\n",
    );
    assert_accept(
        "fn sink(m map[string]int) {}\nfn main() {\n    keep := map[string]int{\"k\": 0}\n    nxt := map[string]int{\"init\": 0}\n    for i in 0..2 {\n        t := nxt\n        nxt = map[string]int{\"next\": i}\n        sink(move t)\n        keep[\"ok\"] = i\n        keep = nxt\n        break\n    }\n}\n",
    );
}

#[test]
fn fresh_non_aliased_flate2_handle_per_iteration_accepts() {
    assert_accept(
        "import \"rust/flate2\"\nfn compress() error {\n    for i in 0..2 {\n        _ = i\n        w := flate2.new_writer(6)?\n        _ = w.write(\"chunk\")?\n        _ = w.finish()?\n    }\n    return nil\n}\nfn main() {}\n",
    );
}
