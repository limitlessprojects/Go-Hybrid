#[test]
fn o8_same_slot_different_radix_literals_rejected_e3102() {
    let src = "struct s { x int }\nfn main() {\nm := map[int]s{10: s{x: 1}}\na := &m[10]\nb := &m[0xA]\nb.x = 2\na.x = 3\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert_eq!(r.verdict, hy_check::Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3102"));
}
