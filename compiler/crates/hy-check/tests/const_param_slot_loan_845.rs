#[test]
fn const_param_slot_and_field_loans_rejected_845() {
    let src = "struct svc { n int }\nstruct holder { s svc }\nconst cm = map[string]svc{\"k\": svc{n: 1}}\nconst cb = holder{s: svc{n: 1}}\nfn m_loc(m map[string]svc) {\ns := &m[\"k\"]\ns.n = 2\n}\nfn m_asn(m map[string]svc) {\nvar s svc\ns = &m[\"k\"]\ns.n = 2\n}\nfn b_loan(b *holder) {\np := &b.s\np.n = 2\n}\nfn main() {\nm_loc(cm)\nm_asn(cm)\nb_loan(&cb)\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", src).unwrap());
    assert_eq!(r.diagnostics.iter().filter(|d| d.code == "HYB-E3006").count(), 3, "{:?}", r.diagnostics);
}
