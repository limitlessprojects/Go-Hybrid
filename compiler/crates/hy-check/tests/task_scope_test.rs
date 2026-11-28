//! Integration tests for #1669: Structured Task Scopes (`HYB-E3501`, `HYB-E3502`, `HYB-E3503`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_src(src: &str) -> (Verdict, Vec<String>) {
    let f = parse_file("test_scope.hy", src).expect("parse ok");
    let r = check_file(&f);
    (r.verdict, r.diagnostics.iter().map(|d| d.code.clone()).collect())
}

#[test]
fn accept_clean_structured_task_scopes() {
    for (idx, src) in [
        "fn work() {}\nfn main() { scope { h := task work()\n join(h) } }",
        "fn work() {}\nfn main() { scope { h := task work()\n h.join() }\n scope s { h2 := task work()\n s.cancel(h2) } }",
        "fn work() {}\nfn main() { ok := true\n scope { h := task work()\n if ok { join(h) } else { cancel(h) } } }",
        "fn work() {}\nfn main() { ch := make(chan int, 1)\n scope { h := task work()\n select { _ := <-ch: { join(h) } default: { cancel(h) } } } }",
        "fn work() {}\nfn main() { scope { h1 := task work()\n scope { h2 := task work()\n drain(h2) }\n join(h1) } }",
        "fn work() {}\nfn main() { scope { h := task work()\n defer join(h) } }",
        "import \"net/http\"\nfn bg() {}\nfn main() { http.handle_func(\"/\", fn(w http.response_writer, r *http.request) { scope { h := task bg()\n join(h) } }) }",
    ].iter().enumerate() {
        let (v, codes) = check_src(src);
        assert_eq!(v, Verdict::Accept, "case {idx} should accept, got {codes:?}");
    }
}

#[test]
fn reject_task_scope_and_affine_violations() {
    for (idx, (code, src)) in [
        ("HYB-E3501", "fn work() {}\nfn main() { scope { h := task work() } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { scope { task work() } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { scope { _ := task work() } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { err := true\n scope { h := task work()\n if err { return }\n join(h) } }"),
        ("HYB-E3501", "fn work() {}\nfn run() int { scope { h := task work()\n return h }\n return 0 }"),
        ("HYB-E3501", "fn work() {}\nfn main() { ch := make(chan int, 1)\n scope { h := task work()\n ch <- move h } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { ok := true\n scope { h := task work()\n if ok { join(h) } } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { ch := make(chan int, 1)\n scope { h := task work()\n select { _ := <-ch: { join(h) } default: {} } } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { for i in 0..2 { scope { h := task work()\n if i == 0 { break }\n join(h) } } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { scope { h1 := task work()\n scope { h2 := task work() }\n join(h1) } }"),
        ("HYB-E3501", "fn work() {}\nfn main() { scope { h := task work()\n h = task work()\n join(h) } }"),
        ("HYB-E3502", "fn work() {}\nfn main() { scope { h := task work()\n join(h)\n join(h) } }"),
        ("HYB-E3502", "fn work() {}\nfn main() { scope { h := task work()\n cancel(h)\n join(h) } }"),
        ("HYB-E3502", "fn work() {}\nfn main() { scope { h := task work()\n h.drain()\n h.cancel() } }"),
        ("HYB-E3502", "fn work() {}\nfn main() { ok := true\n scope { h := task work()\n if ok { join(h) }\n join(h) } }"),
        ("HYB-E3503", "import \"net/http\"\nfn bg() {}\nfn main() { http.handle_func(\"/\", fn(w http.response_writer, r *http.request) { task bg() }) }"),
    ].iter().enumerate() {
        let (v, codes) = check_src(src);
        assert_eq!(v, Verdict::Reject, "case {idx} must reject");
        assert!(codes.iter().any(|c| c == code), "case {idx} expected {code}, got {codes:?}");
    }
}
