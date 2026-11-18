use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("inline.hy", src).expect("parse");
    let r = check_file(&ast);
    let codes = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect();
    (r.verdict, codes)
}

#[test]
fn issue_822_closure_slot_loan_rebind_and_alias_rejected_and_expired_accepted() {
    for src in [
        "struct cell { n int }\nfn main() {\n var m = map[string]cell{\"a\": cell{n: 1}}\n f := fn() {\n p := &m[\"a\"]\n m = map[string]cell{\"b\": cell{n: 2}}\n p.n = 3\n }\n f()\n fmt.println(m[\"a\"].n)\n}\n",
        "struct cell { n int }\nstruct holder { m map[string]cell }\nfn main() {\n var h = holder{m: map[string]cell{\"a\": cell{n: 1}}}\n f := fn() {\n p := &h.m[\"a\"]\n h.m = map[string]cell{\"b\": cell{n: 2}}\n p.n = 3\n }\n f()\n fmt.println(h.m[\"a\"].n)\n}\n",
        "struct cell { n int }\nfn main() {\n m := map[string]cell{\"a\": cell{n: 1}}\n f := fn() {\n p := &m[\"a\"]\n a := m\n a[\"b\"] = cell{n: 2}\n p.n = 3\n }\n f()\n}\n",
    ] {
        let (v, c) = check(src);
        assert_eq!((v, c), (Verdict::Reject, vec!["HYB-E3102".to_string()]));
    }
    let (v, c) = check("struct cell { n int }\nfn main() {\n var m = map[string]cell{\"a\": cell{n: 1}}\n f := fn() {\n p := &m[\"a\"]\n p.n = 3\n a := m\n a[\"b\"] = cell{n: 2}\n m = map[string]cell{\"c\": cell{n: 4}}\n }\n f()\n fmt.println(m[\"c\"].n)\n}\n");
    assert_eq!((v, c), (Verdict::Accept, vec![]));
}

#[test]
fn issue_867_821_closure_scoped_move_and_multilocal_self_shadow() {
    for (src, exp) in [
        ("fn consume(owned map[string]int) { owned[\"a\"] = 2 }\nfn main() {\n x := map[string]int{\"a\": 1}\n if true {\n f := fn() { fmt.println(x[\"a\"]) }\n f()\n }\n task consume(move x)\n}\n", vec!["HYB-E3003"]),
        ("fn consume(owned map[string]int) { owned[\"a\"] = 2 }\nfn main() {\n x := map[string]int{\"a\": 1}\n if true {\n a := x\n f := fn() { fmt.println(a[\"a\"]) }\n f()\n }\n task consume(move x)\n}\n", vec!["HYB-E3003"]),
        ("fn consume(owned map[string]int) { owned[\"k\"] = 9 }\nfn pair(m map[string]int) (map[string]int, map[string]int) { return m, m }\nfn main() {\n f := fn() {\n a := map[string]int{\"k\": 1}\n p := a\n if true {\n a, b := pair(a)\n fmt.println(b[\"k\"])\n task consume(move a)\n }\n fmt.println(a[\"k\"], p[\"k\"])\n }\n f()\n}\n", vec!["HYB-E3003", "HYB-E3003"]),
    ] {
        let (v, c) = check(src);
        assert_eq!((v, c), (Verdict::Reject, exp.into_iter().map(String::from).collect()));
    }
    let (v, c) = check("fn pair(m map[string]int) (map[string]int, map[string]int) { return m, m }\nfn main() {\n x := map[string]int{\"a\": 1}\n if true {\n a := x\n f := fn() {\n m := map[string]int{\"k\": 1}\n if true {\n m, b := pair(m)\n fmt.println(m[\"k\"], b[\"k\"])\n }\n fmt.println(a[\"a\"], m[\"k\"])\n }\n f()\n }\n}\n");
    assert_eq!((v, c), (Verdict::Accept, vec![]));
}