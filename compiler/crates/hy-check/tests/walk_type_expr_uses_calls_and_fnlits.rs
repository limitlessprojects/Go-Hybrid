use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("t.hy", src).expect("parses"))
}

#[test]
fn type_expr_uses_detect_use_after_move_and_extend_slot_loan_window() {
    for tail in [
        "var a [len(arr)]int\n fmt.println(a[0])",
        "x := [cap(arr)]int{0}\n fmt.println(x[0])",
        "x := map[[len(arr)]int]int{}\n fmt.println(len(x))",
        "ch := make(chan [len(arr)]int, 1)\n fmt.println(ch)",
    ] {
        let src = format!(
            "fn worker(a [1]map[string]int) {{ a[0][\"k\"] = 1 }}\n\
             fn main() {{\n  arr := [1]map[string]int{{map[string]int{{\"k\": 0}}}}\n  task worker(move arr)\n  {tail}\n}}"
        );
        let r = check(&src);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3003"), "{tail}: {:?}", r.diagnostics);
    }

    let r_loan = check("fn main() {\n  m := map[string][2]int{\"a\": [2]int{1, 2}}\n  p := &m[\"a\"]\n  m[\"b\"] = [2]int{3, 4}\n  var a [len(p)]int\n  fmt.println(a[0])\n}");
    assert!(r_loan.diagnostics.iter().any(|d| d.code == "HYB-E3102"), "{:?}", r_loan.diagnostics);

    let r_addr = check("fn main() {\n  m := map[string][2]int{\"a\": [2]int{1, 2}}\n  p := &m[\"a\"]\n  var a [len(&p)]int\n  fmt.println(a[0])\n}");
    assert!(r_addr.diagnostics.iter().any(|d| d.code == "HYB-E3104"), "{:?}", r_addr.diagnostics);
}

#[test]
fn type_expr_calls_and_fnlits_enforce_const_loan_and_closure_rules() {
    let r_const = check("const c = map[string]int{\"k\": 0}\nfn mut_map(m map[string]int) int { m[\"k\"] = 1\n return 1 }\nfn main() {\n  var a [mut_map(c)]int\n  fmt.println(a[0])\n}");
    assert!(r_const.diagnostics.iter().any(|d| d.code == "HYB-E3006"), "{:?}", r_const.diagnostics);

    let r_call = check("fn ins(m map[string]int) int { m[\"b\"] = 2\n return 1 }\nfn main() {\n  m := map[string]int{\"a\": 1}\n  p := &m[\"a\"]\n  x := [ins(m)]int{0}\n  fmt.println(p, x[0])\n}");
    assert!(r_call.diagnostics.iter().any(|d| d.code == "HYB-E3102"), "{:?}", r_call.diagnostics);

    let r_fnlit = check("fn worker(m map[string]int) { m[\"k\"] = 1 }\nfn main() {\n  outer := fn() {\n    m := map[string]int{\"k\": 0}\n    var a [(fn() int { task worker(m)\n return 1 })()]int\n    fmt.println(a[0])\n  }\n  outer()\n}");
    assert!(r_fnlit.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{:?}", r_fnlit.diagnostics);

    let r_ok = check("fn main() {\n  m := map[string][2]int{\"a\": [2]int{1, 2}}\n  p := &m[\"a\"]\n  var a [len(p)]int\n  m[\"b\"] = [2]int{3, 4}\n  fmt.println(a[0])\n}");
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
}
