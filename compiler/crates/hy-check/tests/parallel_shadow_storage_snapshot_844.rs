//! Regression tests for #844: parallel-shadow storage snapshot preservation.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn parallel_shadow_844() {
    let hdr = "struct box_m { m map[string]int }\nstruct cell { n int }\n\
        const c = map[string]int{\"k\": 1}\nfn wk(m map[string]int) { _ = len(m) }\n\
        fn wh(b box_m) { _ = len(b.m) }\nfn two(m map[string]int) (int, map[string]int) {\n\
        return 1, m\n}\nfn mp(p map[string]int) {\nif true {\np, a := 1, p\n\
        a[\"k\"] = 9\n_ = p\n}\n}\n\
        fn sw(m map[string]int) (map[string]int, map[string]int) { return map[string]int{}, m }\n";
    let bad = "fn e1() {\nc, a := 1, c\na[\"k\"] = 9\n_ = c\n}\nfn e2() {\na, c := c, 1\n\
        a[\"k\"] = 9\n_ = c\n}\nfn e3() {\nc, h := 1, box_m{m: c}\nh.m[\"k\"] = 9\n_ = c\n}\n\
        fn e4() {\nc, a := two(c)\na[\"k\"] = 9\n_ = c\n}\nfn e5() {\nf := fn() {\n\
        c, a := 1, c\na[\"k\"] = 9\n_ = c\n}\nf()\n}\nfn e6() { mp(c) }\nfn e7() {\n\
        const lc = map[string]int{\"k\": 1}\nlc, a := 1, lc\na[\"k\"] = 9\n_ = lc\n}\n\
        fn e8(p map[string]int) {\nif true {\np, a := 1, p\ntask wk(move a)\n_ = p\n}\n}\n\
        fn e9(p map[string]int) {\na := p\nvar b map[string]int\na, b = sw(a)\n_ = a\n\
        task wk(move b)\n}\nfn e10() {\nm := map[string]cell{\"k\": cell{n: 1}}\ns := &m[\"k\"]\n\
        if true {\nm, a := 1, m\na[\"k2\"] = cell{n: 2}\n_ = m\n}\n_ = s.n\n}\nfn e11() {\n\
        m := map[string]int{\"k\": 1}\na := m\nif true {\n\
        m, f := 1, fn() int { return m[\"k\"] }\ntask wk(move a)\n_ = f()+m\n}\n}\nfn main() {}\n";
    let ok = "const k = 3\nfn main() {\nm := map[string]cell{\"k\": cell{n: 1}}\ns := &m[\"k\"]\n\
        _ = s.n\nif true {\nm, a := 1, m\na[\"k2\"] = cell{n: 2}\n_ = m + len(a)\n}\n\
        it := box_m{m: map[string]int{\"k\": 1}}\nif true {\n\
        it, p := box_m{m: map[string]int{\"k\": 2}}, &it\ntask wh(move it)\n_ = len(p.m)\n}\n\
        k, x := \"s\", k\nx = 9\n_, _ = k, x\n}\n";
    let want = "HYB-E3006 HYB-E3006 HYB-E3006 HYB-E3006 HYB-E3006 HYB-E3006 \
        HYB-E3006 HYB-E3004 HYB-E3004 HYB-E3102 HYB-E3003 HYB-E3003";
    for (w, b) in [(want, bad), ("", ok)] {
        let r = check_file(&parse_file("t.hy", &format!("{hdr}{b}")).expect("parse"));
        let got: Vec<_> = r.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert_eq!(r.verdict == Verdict::Accept, w.is_empty());
        assert_eq!(got, w.split_whitespace().collect::<Vec<_>>());
    }
}
