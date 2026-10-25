use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn rejects_mismatched_multi_value_forwarding_in_task_some_set_fnlit_and_fnfield() {
    for src in [
        "fn pair() (int, int) { return 1, 2 }\nfn w(a int) { _ = a }\nfn main() { task w(pair()) }",
        "fn pair() (int, int) { return 1, 2 }\nfn main() { x := some(pair())\n _ = x }",
        "fn pair() (int, int) { return 1, 2 }\nfn main() { var s set[int]\n s.insert(pair())\n _ = s }",
        "fn pair() (int, int) { return 1, 2 }\nfn main() { (fn(a int) { _ = a })(pair()) }",
        "struct h { cb fn(a int) }\nfn pair() (int, int) { return 1, 2 }\nfn nop(a int) { _ = a }\nfn main() { x := h{cb: nop}\n x.cb(pair()) }",
    ] {
        let r = check_file(&parse_file("test.hy", src).expect("parse"));
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E2002"));
    }
    let ok = check_file(&parse_file("ok.hy", "fn pair() (int, int) { return 1, 2 }\nfn w(a int, b int) { _ = a\n _ = b }\nfn main() { task w(pair())\n (fn(a int, b int) { _ = a\n _ = b })(pair()) }").expect("parse"));
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}
