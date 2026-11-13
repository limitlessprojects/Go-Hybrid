use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const PRE: &str = r#"type mp = map[string]int; fn w(x mp, d chan<- bool) { x["k"] = 1; d <- true }
fn ch() chan int { return make(chan int, 1) }; fn rd(x mp, d chan<- bool) { _ = x["k"]; d <- true }
fn cb0() chan bool { return make(chan bool, 1) }; fn p1() (mp, mp) { x := m0(); return x, x }
fn m0() mp { return map[string]int{"k": 0} }; fn p2() (mp, mp) { return m0(), m0() }
fn p4() (mp, mp, mp, mp) { return m0(), m0(), m0(), m0() }
fn p5a() (int, int, int, mp, mp) { m := m0(); return 0, 0, 0, m, m }
fn p5b() (int, int, int, mp, mp) { return 0, 0, 0, m0(), m0() }"#;
const CPRE: &str = r#"const shared = map[string]int{"k": 0}
const list = []map[string]int{map[string]int{"k": 0}}; fn ps() (mp, mp) { return shared, shared }"#;

fn chk(src: &str, want: &str) {
    let s = format!("{PRE}\n{src}\n").replace("; ", "\n");
    let s = s.replace("{ ", "{\n").replace(" }", "\n}");
    let r = check_file(&parse_file("t.hy", &s).unwrap());
    let empty = r.diagnostics.is_empty() && r.verdict == Verdict::Accept;
    let hit = r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code.ends_with(want));
    assert!(if want == "OK" { empty } else { hit }, "{s}");
}

#[test]
fn return_origin_control_flow_and_multi_return_974() {
    let sp = "d := cb0(); task w(move a, d); b[\"k\"] = 2; <-d";
    let m = |c: &str| format!("fn main() {{ a, b := {c}; {sp} }}");
    let (mp, mf) = (m("pair(false)"), m("f(2)"));
    let cases = r#"p | E3003 | m := m0(); m2 := m; if f { m2 = m0() }; return m, m2
p | OK    | m := m0(); m2 := m; if f { m2 = m0() } else { m2 = m0() }; return m, m2
p | OK    | m := m0(); m2 := m; if f { m2 = m0() } else { return m, m0() }; return m, m2
p | E3003 | return p1()
p | E3003 | p, q := p1(); return p, q
p | E3003 | p := m0(); q := m0(); p, q = p1(); return p, q
p | E3003 | a := m0(); b := m0(); for i in 0..2 { _ = i; b = a; a = m0() }; return a, b
p | E3003 | a, b, c, d := p4(); for i in 0..4 { _ = i; d = c; c = b; b = a; a = m0() }; return c, d
p | E3003 | m := m0(); if false { m := 0; _ = m }; return m, m
p | E3003 | m := m0(); c := ch(); select { m := <-c: { _ = m }; default: {} }; return m, m
p | E3003 | m := m0(); o := some(0); match o { some(m): { _ = m }; none: {} }; return m, m
p | E3003 | m := m0(); m2 := m; g := fn() (mp, mp) { return m, m2 }; return g()
p | E3003 | m := m0(); m2 := m; g := fn(x mp) (mp, mp) { return m, x }; return g(m2)
p | OK    | m := m0(); g := fn(x mp) (mp, mp) { _ = x; return p2() }; return g(m)
p | E3003 | cb := fn() (int, int, int, mp, mp) { return p5a() }; \
    if f { cb = fn() (int, int, int, mp, mp) { return p5b() } }; _, _, _, x, y := cb(); return x, y
n | E3003 | defer fn() { r1, r2 = p1() }(); return p2()
n | E3003 | defer fn() { x, y := p1(); r1 = x; r2 = y }(); return p2()
n | OK    | defer fn() { r1 = m0(); r2 = m0() }(); return p1()
n | E3003 | if f { defer fn() { r1 = m0(); r2 = m0() }() }; return p1()
g | E3006 | for _, v in list { return v }; return m0()
g | E3006 | if f { shared := 0; _ = shared }; return shared
g | OK    | shared := m0(); return shared
g | E3006 | m := m0(); if f { m = shared } else { m = m0() }; return m
q | E3004 | if f { shared := 0; _ = shared }; return shared
c | E3003 | shared := 0; _ = shared; a, b := ps()
r | E3003 | if n > 0 { p, q := rec(n - 1); return p, q }; return p1()
r | OK    | if n > 0 { p, q := rec(n - 1); return p, q }; return p2()
u | E3003 | if n <= 0 { return p1() }; return g(n - 1) || return f(n - 1)
u | E3003 | return g(n - 1) || if n <= 0 { return p1() }; return f(n - 1)
u | OK    | return g(n - 1) || if n <= 0 { return p2() }; return f(n - 1)
m | E3003 | m := m0(); m2 := m; g := fn(x mp) (mp, mp) { return m, x }; a, b := g(m2)
x | E3003 | a, _, _, _, b := cb()
x | E3003 | _, _, _, a, b := cb()"#;
    for line in cases.replace("\\\n", "").lines() {
        let mut p = line.splitn(3, '|').map(str::trim);
        let (k, w, b) = (p.next().unwrap(), p.next().unwrap(), p.next().unwrap());
        let (x, y) = b.split_once("||").unwrap_or((b, ""));
        let gp = format!("{CPRE}; fn get(f bool) mp {{ _ = f; {b} }}; fn main()");
        let src = match k {
            "p" => format!("fn pair(f bool) (mp, mp) {{ _ = f; {b} }}; {mp}"),
            "n" => format!("fn pair(f bool) (r1 mp, r2 mp) {{ _ = f; {b} }}; {mp}"),
            "g" => format!("{gp} {{ g := get(false); g[\"k\"] = 2 }}"),
            "q" => format!("{gp} {{ d := cb0(); g := get(false); task rd(move g, d); <-d }}"),
            "c" => format!("{CPRE}; fn main() {{ {b}; {sp} }}"),
            "r" => format!("fn rec(n int) (mp, mp) {{ {b} }}; {}", m("rec(2)")),
            "u" => format!("fn f(n int) (mp, mp) {{ {x} }}; fn g(n int) (mp, mp) {{ {y} }}; {mf}"),
            "m" => format!("fn main() {{ {b}; {sp} }}"),
            _ => format!("fn run(cb fn() mp mp mp mp mp) {{ {b}; {sp} }}; fn main() {{}}"),
        };
        chk(&src, w);
    }
    let mut deep = format!("fn main() {{ a, _, _, _, b := h0(); {sp} }}\n");
    deep += "fn h34() (mp, mp, mp, mp, mp) { m := m0(); return m, m0(), m0(), m0(), m }\n";
    for i in 0..34 {
        deep += &format!("fn h{i}() (mp, mp, mp, mp, mp) {{ return h{}() }}\n", i + 1);
    }
    chk(&deep, "E3003");
}
