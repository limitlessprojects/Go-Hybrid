//! Soundness regressions for bound method values and indirect closure callees
//! across O5.2 alias freeze (`HYB-E3003`), O5.1/O5.4 non-owned escape (`HYB-E3004`),
//! auto-addressed pointer-receiver borrows, and A3 map-slot loan exclusivity (`HYB-E3102`).
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_reject_code(src: &str, code: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject ({code}) for:\n{src}");
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got {:?} for:\n{src}",
        r.diagnostics
    );
}

fn assert_accept(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?} for:\n{src}", r.diagnostics);
}

const PRE: &str = "struct box { data []int }\nstruct gbox[t any] { data t }\nstruct counter { n int }\nstruct entry { n int }\nstruct evicter { m map[string]entry }\nstruct hook { run fn() }\nfn (b box) get() []int { return b.data }\nfn (b box) count() int { return len(b.data) }\nfn (b *box) put(x []int) { b.data = x }\nfn (g gbox[t]) get() t { return g.data }\nfn (c *counter) ptr() *counter { return c }\nfn (c *counter) add(d int) { c.n = c.n + d }\nfn (e evicter) drop_key() { delete(e.m, \"k\") }\nfn (e evicter) peek() int { return len(e.m) }\nfn make_put(b *box) fn(x []int) { return fn(x []int) { b.data = x } }\nfn make_drop(m map[string]entry) fn() { return fn() { delete(m, \"k\") } }\nfn apply_put(b *box, x []int) { f := b.put\nf(x) }\nfn w_slice(s []int, ch chan int) { s[0] = 9\nch <- 1 }\nfn w_box(b box, ch chan int) { b.data[0] = 9\nch <- 1 }\nfn w_ptr(p *counter, ch chan int) { p.n = 9\nch <- 1 }\n";

#[test]
fn bound_method_value_return_alias_and_non_owned_escape_reject() {
    assert_reject_code(&format!("{PRE}fn main() {{\norig := []int{{1, 2}}\nb := box{{data: orig}}\ng := b.get\ns := g()\nch := make(chan int, 1)\ntask w_slice(move s, ch)\norig[0] = 7\n_ = <-ch\n}}\n"), "HYB-E3003");
    assert_reject_code(&format!("{PRE}fn main() {{\norig := []int{{1, 2}}\nb := gbox[[]int]{{data: orig}}\ng := b.get\ns := g()\nch := make(chan int, 1)\ntask w_slice(move s, ch)\norig[0] = 7\n_ = <-ch\n}}\n"), "HYB-E3003");
    assert_reject_code(&format!("{PRE}fn leak(b box) {{\ng := b.get\ns := g()\nch := make(chan int, 1)\ntask w_slice(move s, ch)\n_ = <-ch\n}}\nfn main() {{\nleak(box{{data: []int{{1}}}})\n}}\n"), "HYB-E3004");
    assert_reject_code(&format!("{PRE}fn main() {{\nvar c counter\nf := c.ptr\np := f()\nch := make(chan int, 1)\ntask w_ptr(move p, ch)\nc.n = 4\n_ = <-ch\n}}\n"), "HYB-E3003");
}

#[test]
fn indirect_callee_argument_smuggle_into_captured_storage_rejects() {
    assert_reject_code(&format!("{PRE}fn main() {{\norig := []int{{1, 2}}\nb := box{{data: []int{{0}}}}\nf := make_put(&b)\nf(orig)\nch := make(chan int, 1)\ntask w_box(move b, ch)\norig[0] = 7\n_ = <-ch\n}}\n"), "HYB-E3003");
    assert_reject_code(&format!("{PRE}fn main() {{\norig := []int{{1, 2}}\nb := box{{data: []int{{0}}}}\nf := b.put\nf(orig)\nch := make(chan int, 1)\ntask w_box(move b, ch)\norig[0] = 7\n_ = <-ch\n}}\n"), "HYB-E3003");
    assert_reject_code(&format!("{PRE}fn main() {{\norig := []int{{1, 2}}\nb := box{{data: []int{{0}}}}\napply_put(&b, orig)\nch := make(chan int, 1)\ntask w_box(move b, ch)\norig[0] = 7\n_ = <-ch\n}}\n"), "HYB-E3003");
    assert_reject_code(&format!("{PRE}fn leak(orig []int) {{\nb := box{{data: []int{{0}}}}\nf := make_put(&b)\nf(orig)\nch := make(chan int, 1)\ntask w_box(move b, ch)\n_ = <-ch\n}}\nfn main() {{\nleak([]int{{1, 2}})\n}}\n"), "HYB-E3004");
}

#[test]
fn bound_method_value_and_container_closure_a3_slot_loan_reject() {
    assert_reject_code(&format!("{PRE}fn main() {{\nm := map[string]entry{{\"k\": entry{{n: 1}}}}\ne := evicter{{m: m}}\nf := e.drop_key\np := &m[\"k\"]\nf()\np.n = 2\n}}\n"), "HYB-E3102");
    assert_reject_code(&format!("{PRE}fn main() {{\nm := map[string]entry{{\"k\": entry{{n: 1}}}}\nfs := []fn(){{fn() {{ delete(m, \"k\") }}}}\np := &m[\"k\"]\nfs[0]()\np.n = 2\n}}\n"), "HYB-E3102");
    assert_reject_code(&format!("{PRE}fn main() {{\nm := map[string]entry{{\"k\": entry{{n: 1}}}}\nh := hook{{run: make_drop(m)}}\np := &m[\"k\"]\nh.run()\np.n = 2\n}}\n"), "HYB-E3102");
}

#[test]
fn scalar_method_values_and_non_mutating_a3_calls_accept() {
    assert_accept(&format!("{PRE}fn main() {{\nb := box{{data: []int{{1, 2}}}}\nf := b.count\nn := f()\nch := make(chan int, 1)\ntask w_box(move b, ch)\n_ = n\n_ = <-ch\n}}\n"));
    assert_accept(&format!("{PRE}fn main() {{\nm := map[string]entry{{\"k\": entry{{n: 1}}}}\ne := evicter{{m: m}}\nf := e.peek\np := &m[\"k\"]\n_ = f()\np.n = 2\n}}\n"));
    assert_accept(&format!("{PRE}fn main() {{\nvar c counter\nf := c.add\nf(3)\np := &c\nch := make(chan int, 1)\ntask w_ptr(move p, ch)\n_ = <-ch\n}}\n"));
}
