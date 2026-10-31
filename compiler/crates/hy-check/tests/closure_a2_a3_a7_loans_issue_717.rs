use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn loop_and_closure_enforce_o8_a3_o7_a2_and_a7_loan_windows() {
    let r_o8_loop_and_closure = check("import \"fmt\"\nstruct svc { n int }\nfn main() {\nm := map[string]svc{\"k\": svc{n: 1}}\nfor i in 0..2 {\nfmt.println(i)\ns := &m[\"k\"]\nfmt.println(s.n)\nm[\"k\"].n = 2\n}\nf := fn() {\ns1 := &m[\"k\"]\ns2 := &m[\"k\"]\nm[\"k\"].n = 3\nfmt.println(s1.n, s2.n)\n}\nf()\n}");
    assert_eq!(r_o8_loop_and_closure.verdict, Verdict::Reject, "{:?}", r_o8_loop_and_closure.diagnostics);
    let e3102_count = r_o8_loop_and_closure.diagnostics.iter().filter(|d| d.code == "HYB-E3102").count();
    assert_eq!(e3102_count, 3, "{:?}", r_o8_loop_and_closure.diagnostics);
    let r_closure_a2_and_a7 = check("import \"fmt\"\nimport \"rust/serde_json\"\nimport \"rust/flate2\"\nfn main() {\nf_a2 := fn() (int, error) {\ndoc := serde_json.parse(\"[]\")?\nu := serde_json.array(doc, \"users\")?\n_ = serde_json.set(doc, \"pruned\", true)?\nreturn u.len(), nil\n}\n_, _ = f_a2()\nf_a7 := fn() error {\nenc := flate2.new_writer(6)?\na, b := enc.finish()?, enc.write([]byte(\"x\"))?\nfmt.println(len(a), b)\nreturn nil\n}\n_ = f_a7()\n}");
    assert_eq!(r_closure_a2_and_a7.verdict, Verdict::Reject, "{:?}", r_closure_a2_and_a7.diagnostics);
    assert!(r_closure_a2_and_a7.diagnostics.iter().any(|d| d.code == "HYB-E3101"));
    assert!(r_closure_a2_and_a7.diagnostics.iter().any(|d| d.code == "HYB-E3003" && d.tier0_rule.as_deref() == Some("A7")));
}
