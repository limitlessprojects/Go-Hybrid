use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn rejects_multi_value_calls_in_match_guards_and_closure_if_init_conditions() {
    for src in [
        "fn pair() (int, int) { return 1, 2 }\nfn main() {\n opt := some(pair)\n match opt { some(f) if f() == 1: {} none: {} }\n}",
        "fn single() int { return 1 }\nfn pair() (int, int) { return 1, 2 }\nfn main() {\n f := single\n opt := some(pair)\n match opt { some(f) if f() == 1: {} none: {} }\n}",
        "fn single() int { return 1 }\nfn pair() (int, int) { return 1, 2 }\nfn main() {\n f := single\n cb := fn() { if f := pair; f() == 1 {} }\n cb()\n}",
    ] {
        let r = check_file(&parse_file("test.hy", src).expect("parse"));
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E2002"));
    }
    let ok = check_file(&parse_file("ok.hy", "fn single() int { return 1 }\nfn pair() (int, int) { return 1, 2 }\nfn main() {\n f := pair\n opt := some(single)\n match opt { some(f) if f() == 1: {} none: {} }\n cb := fn() { if f := single; f() == 1 {} }\n cb()\n _ = f\n}").expect("parse"));
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}
