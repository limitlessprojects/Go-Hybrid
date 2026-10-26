use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
fn check(src: &str) -> hy_check::CheckResult { check_file(&parse_file("test.hy", src).unwrap()) }
#[test]
fn move_and_multi_assign_preserve_o7_a2_and_o8_a3_loan_windows() {
    let r_slot_move_define = check("import \"fmt\"\nstruct svc { n int }\nfn main() {\nm := map[string]svc{\"k\": svc{n: 1}}\ns := &m[\"k\"]\np := move s\nm[\"other\"] = svc{n: 2}\nfmt.println(p.n)\n}");
    assert_eq!(r_slot_move_define.verdict, Verdict::Reject, "{:?}", r_slot_move_define.diagnostics);
    assert!(r_slot_move_define.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
    let r_slot_move_assign = check("import \"fmt\"\nstruct svc { n int }\nfn main() {\nm := map[string]svc{\"k\": svc{n: 1}}\ns := &m[\"k\"]\np := &m[\"z\"]\nfmt.println(p.n)\np = move s\ndelete(m, \"other\")\nfmt.println(p.n)\n}");
    assert_eq!(r_slot_move_assign.verdict, Verdict::Reject, "{:?}", r_slot_move_assign.diagnostics);
    assert!(r_slot_move_assign.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
    let r_view_multi_assign = check("import \"rust/serde_json\"\nfn pair(x serde_json.view) (int, serde_json.view) {\nreturn 1, x\n}\nfn bad() (int, error) {\ndoc := serde_json.parse(\"[]\")?\nu := serde_json.array(doc, \"users\")?\nv := serde_json.array(doc, \"other\")?\n_ = v.len()\nn := 0\nn, v = pair(u)\n_ = serde_json.set(doc, \"pruned\", true)?\nreturn n + v.len(), nil\n}\nfn main() {\n_, _ = bad()\n}");
    assert_eq!(r_view_multi_assign.verdict, Verdict::Reject, "{:?}", r_view_multi_assign.diagnostics);
    assert!(r_view_multi_assign.diagnostics.iter().any(|d| d.code == "HYB-E3101"));
}
