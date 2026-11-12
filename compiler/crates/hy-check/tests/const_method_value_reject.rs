use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn assert_e3006(src: &str, single: bool, what: &str) {
    let r = check_file(&parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse: {:?}", d)));
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect();
    assert_eq!(r.verdict, Verdict::Reject, "{}: {:?}", what, cs);
    if single { assert_eq!(cs, vec!["HYB-E3006".to_string()], "{}", what); } else { assert!(cs.iter().any(|c| c == "HYB-E3006"), "{}: {:?}", what, cs); }
}
const PRE: &str = "struct holder { m map[string]int }\nfn (h holder) stomp(v int) { h.m[\"k\"] = v }\nconst shared = holder{m: map[string]int{\"k\": 1}}\nstruct counter { n int }\nfn (c *counter) inc(v int) { c.n = c.n + v }\nfn (c *counter) bump_bool() bool {\n    c.n = c.n + 1\n    return true\n}\nfn (c *counter) bump_int() int {\n    c.n = c.n + 1\n    return 0\n}\nfn (c *counter) bump_chan() chan int {\n    c.n = c.n + 1\n    return make(chan int, 1)\n}\nconst cnt = counter{n: 0}\n";
#[test]
fn direct_closure_interproc_and_escape_positions_are_e3006() {
    for (what, extra) in [
        ("1A local", "fn main() {\n    f := shared.stomp\n    f(2)\n}\n"), ("1A assign", "fn main() {\n    var f fn(int)\n    f = shared.stomp\n    f(2)\n}\n"), ("1B ptr-recv", "fn main() {\n    f := cnt.inc\n    f(1)\n}\n"), ("fn-local const", "fn main() {\n    const lc = counter{n: 0}\n    f := lc.inc\n    f(1)\n}\n"),
        ("paren callee", "fn main() { (cnt.inc)(1) }\n"), ("fn call arg", "fn run(f fn(int)) { f(1) }\nfn main() { run(shared.stomp) }\n"), ("closure arg", "fn main() {\n    cb := fn(f fn(int)) { f(1) }\n    cb(shared.stomp)\n}\n"), ("return value", "fn leak() fn(int) { return cnt.inc }\nfn main() { leak()(1) }\n"),
        ("struct lit", "struct bf { f fn(int) }\nfn main() { _ = bf{f: cnt.inc} }\n"), ("map lit", "fn main() { _ = map[string]fn(int){\"a\": shared.stomp} }\n"), ("slice iterate", "fn main() { for _, f in []fn(int){cnt.inc} { f(1) } }\n"), ("file const lit", "const fns = []fn(int){cnt.inc}\nfn main() { fns[0](1) }\n"),
        ("closure capture", "fn main() {\n    g := fn() {\n        f := cnt.inc\n        f(1)\n    }\n    g()\n}\n"), ("nested+alias", "type ac = counter\nstruct outer { inner ac }\nconst top = outer{inner: counter{n: 0}}\nfn main() {\n    f := top.inner.inc\n    f(1)\n}\n"),
        ("ESC_13 match main", "struct oopt { opt option[holder] }\nconst wrap = oopt{opt: some(holder{m: map[string]int{\"k\": 1}})}\nfn main() {\n    match wrap.opt {\n        some(v) => {\n            f := v.stomp\n            f(99)\n        }\n        none => {}\n    }\n}\n"), ("ESC_13 match closure", "struct oopt { opt option[holder] }\nconst wrap = oopt{opt: some(holder{m: map[string]int{\"k\": 1}})}\nfn main() {\n    g := fn() {\n        match wrap.opt {\n            some(v) => {\n                f := v.stomp\n                f(99)\n            }\n            none => {}\n        }\n    }\n    g()\n}\n"),
        ("BYPASS_1", "struct unrelated { stomp int }\nfn make_mut(h holder) fn(int) { return h.stomp }\nfn main() { _ = make_mut(shared) }\n"), ("BYPASS_2", "struct unrelated { inc int }\ntype pc = *counter\nfn via_p(c pc) {\n    f := c.inc\n    f(42)\n}\nfn main() { via_p(cnt) }\n"), ("ESC_15 fnlit", "struct unrelated { inc int }\nfn main() {\n    (fn(c *counter) {\n        f := c.inc\n        f(99)\n    })(cnt)\n}\n"),
        ("REJ_01 file const val", "fn make_mut(h holder) fn(int) { return h.stomp }\nconst fns = []fn(int){make_mut(shared)}\nfn main() { fns[0](9) }\n"), ("REJ_02 file const ptr", "fn make_inc(c *counter) fn(int) { return c.inc }\nconst fns = []fn(int){make_inc(cnt)}\nfn main() { fns[0](9) }\n"), ("post-rebind clos slice", "const cells = [7]\nfn main() {\n    s := [1]\n    cl := fn() { s[0] = 99 }\n    s = cells\n    cl()\n}\n"),
        ("post-rebind clos map", "fn main() {\n    x := holder{m: map[string]int{\"k\": 0}}\n    cl := fn() { x.m[\"k\"] = 99 }\n    x = shared\n    cl()\n}\n"), ("post-rebind clos mval", "fn main() {\n    x := holder{m: map[string]int{\"k\": 0}}\n    cl := fn() {\n        f := x.stomp\n        f(99)\n    }\n    x = shared\n    cl()\n}\n"), ("post-rebind local const", "fn main() {\n    x := holder{m: map[string]int{\"k\": 0}}\n    cl := fn() {\n        f := x.stomp\n        f(99)\n    }\n    const lc = holder{m: map[string]int{\"k\": 7}}\n    x = lc\n    cl()\n}\n"),
    ] { assert_e3006(&format!("{PRE}\n{extra}"), true, what); }
    for (what, hbody) in [
        ("REJ_03 iterate", "for _, f in []fn(int){c.inc} { f(1) }"), ("ESC_01 if init+cond", "if a := c; ([]fn() bool{a.bump_bool})[0]() {}"), ("ESC_02 for while", "for ([]fn() bool{c.bump_bool})[0]() { break }"), ("ESC_03 for range", "for i in 0..([]fn() int{c.bump_int})[0]() { _ = i }"),
        ("ESC_04 match subj", "match some(([]fn() int{c.bump_int})[0]()) { _ => {} }"), ("ESC_05 match guard", "match some(1) { some(v) if ([]fn() bool{c.bump_bool})[0]() => { _ = v } _ => {} }"), ("ESC_06 assign idx", "arr := []int{0}\n    arr[([]fn() int{c.bump_int})[0]()] = 1"), ("ESC_07 slice", "arr := []int{0, 1}\n    _ = arr[0:([]fn() int{c.bump_int})[0]()]"),
        ("ESC_08 interp", "_ = \"v={([]fn() int{c.bump_int})[0]()}\""), ("ESC_09a send", "ch := make(chan fn(int), 1)\n    ch <- c.inc"), ("ESC_09b task", "task (fn(f fn(int)) { f(1) })(c.inc)"), ("ESC_10 select send", "ch := make(chan fn(int), 1)\n    select { ch <- c.inc => {} default => {} }"), ("ESC_11 select recv", "select { x := <-([]fn() chan int{c.bump_chan})[0]() => { _ = x } default => {} }"), ("ESC_12 expr recv", "_ = <-([]fn() chan int{c.bump_chan})[0]()"),
    ] { assert_e3006(&format!("{PRE}\nstruct unrelated {{ inc int }}\nfn h(c *counter) {{\n    {hbody}\n}}\nfn main() {{ h(cnt) }}\n"), false, what); }
    assert_e3006(&format!("{PRE}\nfn run(f fn(int)) {{ f(1) }}\nfn main() {{ task run(cnt.inc) }}\n"), false, "task arg");
    assert_e3006(&format!("{PRE}\nfn main() {{\n    ch := make(chan fn(int), 1)\n    ch <- cnt.inc\n}}\n"), false, "chan send");
    assert!(check_file(&parse_file("inline.hy", &format!("{PRE}\nfn main() {{\n    x := holder{{m: map[string]int{{\"k\": 0}}}}\n    for i in 0..2 {{\n        f := x.stomp\n        f(i)\n        x = shared\n    }}\n}}\n")).unwrap()).diagnostics.iter().any(|d| d.code == "HYB-E3006"), "back-edge");
    let _ = check_file(&parse_file("inline.hy", &format!("{PRE}\ntype la = lb\ntype lb = la\nfn bad(x la) {{ _ = x.inc }}\nfn main() {{}}\n")).unwrap());
}
