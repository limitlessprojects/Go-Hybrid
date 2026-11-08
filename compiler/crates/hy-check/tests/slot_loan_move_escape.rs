#[test]
fn moving_slot_loan_rejects_e3104_and_detached_copy_accepts() {
    for (b, want_err) in [("var s *acct; s = &m[\"a\"]; task w(move s)", true), ("var s *acct; s = &m[\"a\"]; ch <- move s", true), ("s := &m[\"a\"]; task w(move s)", true), ("var a [2]int; p := &clone_arr(a)[:][0]; task w_int(move p); a[0] = 1", false)] {
        let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", &format!("struct acct {{ n int }}\nfn clone_arr(a [2]int) [2]int {{ return a }}\nfn w(p *acct) {{ p.n = 1 }}\nfn w_int(p *int) {{}}\nfn main() {{\n    m := map[string]acct{{\"a\": acct{{n: 1}}}}\n    ch := make(chan *acct, 1)\n    {}\n    _ = m\n    _ = ch\n}}\n", b.replace("; ", "\n    "))).unwrap());
        assert_eq!(r.diagnostics.iter().any(|d| d.code == "HYB-E3104"), want_err, "{b}: {:?}", r.diagnostics);
        // The detached-copy case slices an unaddressable call result, which Go rejects (#2789: HYB-E1002); it must still not be a slot loan.
        if !want_err { assert!(r.diagnostics.iter().all(|d| d.code == "HYB-E1002"), "{b}: {:?}", r.diagnostics); }
    }
}
