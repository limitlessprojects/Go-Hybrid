//! Regression tests for #1601: non-composite literals fail closed (`HYB-E1002`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity::Error;
use hy_syntax::parse_file;

fn check(n: usize, src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("parse"));
    let ds = &r.diagnostics;
    let codes: Vec<_> = ds
        .iter()
        .filter(|d| d.severity == Error)
        .map(|d| d.code.as_str())
        .collect();
    assert_eq!(r.verdict == Verdict::Accept, n == 0, "{src}");
    assert_eq!(codes, vec!["HYB-E1002"; n]);
}

#[test]
fn literals_1601() {
    let bad = "import \"io\"\nimport \"net/http\"\nimport \"path/filepath\"\nimport \"time\"\n\
        struct point { x int }\nstruct box[t any] { v t }\ntype p_pt = *point\n\
        type fn_t = fn(int) int\ntype ch_t = chan int\ntype rdr = io.reader\n\
        type a0 = int\ntype a1 = a0\ntype a2 = a1\ntype a3 = a2\ntype a4 = a3\ntype a5 = a4\n\
        type a6 = a5\ntype a7 = a6\ntype a8 = a7\ntype a9 = a8\ntype a10 = a9\nconst bad = int{}\n\
        fn (b box[t]) m() {\n x := t{}\n _ = x\n}\nfn g[t comparable]() {\n x := t{}\n _ = x\n}\n\
        fn h[point any]() {\n p := point{}\n _ = p\n}\nfn main() {\n \
        a, b, c, d, e, f, g := int{}, string{}, bool{}, f64{a: 1}, any{}, u8{}, byte{}\n \
        k, l, m, o, p := rune{}, complex128{}, error{}, comparable{}, int[int]{}\n \
        q, r, s, u, v := comparable[int]{}, a10{}, p_pt{}, fn_t{}, ch_t{}\n \
        w, x, y, z := io.reader{}, http.handler_func{}, rdr{}, http.dir{}\n \
        d1, d2 := filepath.walk_func{}, time.duration{}\n \
        _, _, _, _, _, _, _, _, _, _, _, _ = a, b, c, d, e, f, g, k, l, m, o, p\n \
        _, _, _, _, _, _, _, _, _, _, _, _ = q, r, s, u, v, w, x, y, z, d1, d2, bad\n}\n";
    let ok1 = "struct int { v u8 }\nstruct any {}\nstruct error {}\nstruct comparable {}\n\
        type my_err = error\ntype my_comp = comparable\nfn main() {\n \
        x, a, e, c, me, mc := int{v: 1}, any{}, error{}, comparable{}, my_err{}, my_comp{}\n \
        _, _, _, _, _, _ = x.v, a, e, c, me, mc\n}\n";
    let ok2 = "import \"sync\"\nimport \"sync/atomic\"\nstruct point { x int }\n\
        struct box[t any] { v t }\ntype pt = point\ntype ints = []int\ntype dict = map[string]int\n\
        type pair = [2]int\nfn mk[k comparable, v any]() {\n \
        m, s, o, st := map[k]v{}, []v{}, option[v]{}, set[k]{}\n _, _, _, _ = m, s, o, st\n}\n\
        fn main() {\n p, q, b, s := pt{x: 3}, &point{x: 4}, box[int]{v: 5}, ints{}\n \
        m, a, o, st := dict{}, pair{}, option[int]{}, set[int]{}\n \
        mu, at := sync.mutex{}, atomic.int64{}\n _ = p.x + q.x + b.v + len(s) + len(m) + len(a)\n \
        _ = o.is_some()\n _, _, _ = st, mu, at\n}\n";
    check(27, bad);
    check(0, ok1);
    check(0, ok2);
}
