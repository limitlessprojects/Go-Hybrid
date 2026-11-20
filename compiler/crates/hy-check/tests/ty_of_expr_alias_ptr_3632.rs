use hy_check::{check_file, Verdict};
#[test]
fn ty_of_expr_and_multi_value_tys_resolve_aliases_and_pointers_for_e6002_3632() {
    let base = "type byte_slice = []u8\ntype byte_map = map[string]u8\n";
    for body in ["xs := make(byte_slice, 1)\n xs[0] = 300\n _ = xs", "m2 := make(byte_map)\n m2[\"a\"] = 300\n _ = m2", "x := p[0]\n x = 300\n _ = x", "v, ok := m[\"a\"]\n v = 300\n _ = v\n _ = ok"] {
        let r = check_file(&hy_syntax::parse_file("test.hy", &format!("{base}fn check(p *[2]u8, m byte_map) {{ _ = p\n _ = m\n {body} }}\nfn main() {{}}\n")).unwrap());
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{body}: {:?}", r.diagnostics);
    }
}
