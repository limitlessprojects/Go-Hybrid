//! Regression tests for A5 captured-write closure escapes (`HYB-E3013`,
//! SPEC-LANGUAGE §8.5 C9 / GUARANTEES.md rows 5 & 9) across `for ... in`
//! element iteration, direct channel receives (`<-ch`), multi-return
//! destructuring, sub-slice projections, and fallible `?` unwrapping, plus
//! channel type alias composite literals and shadowed `make` targets (`HYB-E2005`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn assert_rejects_code(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject ({code}), got {:?} for:\n{src}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got {:?} for:\n{src}",
        r.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept, got {:?} for:\n{src}",
        r.diagnostics
    );
}

const PRE: &str = "import \"net/http\"\n\
import \"time\"\n\
type cb_fn = fn()\n\
type reg_fn = fn(p string, cb fn())\n\
struct stats { n int }\n\
struct holder { m map[string]int }\n\
fn _use_pkgs() {\n\
    _ = time.second\n\
    http.handle_func(\"/k\", fn() {})\n\
}\n\
fn (s *stats) incr() { s.n++ }\n\
fn (s *stats) report() { _ = s.n }\n\
fn (h holder) stomp() { h.m[\"k\"] = 1 }\n\
fn wipe_all(bs []map[string]int) {\n\
    for _, b in bs { b[\"k\"] = 1 }\n\
}\n\
fn pair_ptr(s *stats) (int, *stats) { return 1, s }\n\
fn pair_map(m map[string]int) (int, map[string]int) { return 1, m }\n\
fn make_cb_err(s *stats) (cb_fn, error) { return s.incr, nil }\n\
fn local_reg(p string, cb fn()) { _ = p\n cb() }\n\
fn pick_reg_err() (reg_fn, error) { return http.handle_func, nil }";

#[test]
fn for_iterate_over_captured_containers_in_escaping_closure_rejects_e3013() {
    for body in [
        "s := stats{n: 0}\nptrs := []*stats{&s}\n_ = time.after_func(0, fn() {\nfor _, p in ptrs { p.n = 1 }\n})",
        "s := stats{n: 0}\nptrs := []*stats{&s}\n_ = time.after_func(0, fn() {\nfor _, p in ptrs { p.incr() }\n})",
        "buckets := []map[string]int{map[string]int{\"k\": 0}}\n_ = time.after_func(0, fn() {\nfor _, b in buckets { b[\"k\"] = 1 }\n})",
        "buckets := []map[string]int{map[string]int{\"k\": 0}}\n_ = time.after_func(0, fn() {\nfor _, b in buckets { delete(b, \"k\") }\n})",
        "buckets := []map[string]int{map[string]int{\"k\": 0}}\n_ = time.after_func(0, fn() {\nalias := buckets\nfor _, b in alias { clear(b) }\n})",
        "buckets := []map[string]int{map[string]int{\"k\": 0}}\n_ = time.after_func(0, fn() {\nwipe_all(buckets)\n})",
        "groups := map[string][]int{\"g\": []int{0}}\n_ = time.after_func(0, fn() {\nfor _, xs in groups { xs[0] = 9 }\n})",
        "h := holder{m: map[string]int{\"k\": 0}}\nholders := []holder{h}\n_ = time.after_func(0, fn() {\nfor _, it in holders { it.stomp() }\n})",
    ] {
        assert_rejects_code(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"), "HYB-E3013");
    }
}

#[test]
fn direct_channel_recv_and_multi_return_destructuring_reject_e3013() {
    for body in [
        "ch := make(chan map[string]int, 1)\n_ = time.after_func(0, fn() {\nm := <-ch\nm[\"k\"] = 1\n})",
        "ch := make(chan *stats, 1)\n_ = time.after_func(0, fn() {\np, ok := <-ch\n_ = ok\np.n = 2\n})",
        "ch := make(chan holder, 1)\n_ = time.after_func(0, fn() {\nh := <-ch\n_ = time.after_func(0, h.stomp)\n})",
        "ch := make(chan map[string]int, 1)\n_ = time.after_func(0, fn() {\nselect {\nm := <-ch:\nm[\"k\"] = 1\n}\n})",
        "ch := make(chan map[string]int, 1)\n_ = time.after_func(0, fn() {\nfor m in ch { m[\"k\"] = 1 }\n})",
        "s := stats{n: 0}\n_ = time.after_func(0, fn() {\n_, p := pair_ptr(&s)\np.n = 3\n})",
        "m := map[string]int{\"k\": 0}\n_ = time.after_func(0, fn() {\n_, a := pair_map(m)\na[\"k\"] = 4\n})",
        "s := stats{n: 0}\n_ = time.after_func(0, fn() {\nok := 0\nls := stats{n: 1}\np := &ls\nok, p = pair_ptr(&s)\n_ = ok\np.n = 5\n})",
    ] {
        assert_rejects_code(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"), "HYB-E3013");
    }
}

#[test]
fn subslice_and_try_unwrapped_callbacks_and_foreign_callees_reject_e3013() {
    for body in [
        "s := stats{n: 0}\ncbs := []fn(){s.report, s.incr}\nsub := cbs[1:]\nhttp.handle_func(\"/a\", sub[0])\n_ = s\nreturn nil",
        "s := stats{n: 0}\ncbs := []fn(){s.report, s.incr}\nhttp.handle_func(\"/a\", cbs[1:][0])\n_ = s\nreturn nil",
        "s := stats{n: 0}\nhttp.handle_func(\"/a\", make_cb_err(&s)?)\nreturn nil",
        "s := stats{n: 0}\ncb := make_cb_err(&s)?\nhttp.handle_func(\"/a\", cb)\nreturn nil",
        "m := map[string]int{\"k\": 0}\n(pick_reg_err()?)(\"/x\", fn() { m[\"k\"] = 1 })\nreturn nil",
        "m := map[string]int{\"k\": 0}\nxs := []reg_fn{local_reg, http.handle_func}\nsub := xs[1:]\nsub[0](\"/x\", fn() { m[\"k\"] = 1 })\nreturn nil",
    ] {
        assert_rejects_code(
            &format!("{PRE}\nfn run() error {{\n{body}\n}}\nfn main() {{ _ = run() }}\n"),
            "HYB-E3013",
        );
    }
}

#[test]
fn value_isolated_iteration_readonly_subslice_and_managed_stay_accepted() {
    for body in [
        "vals := []stats{stats{n: 0}}\n_ = time.after_func(0, fn() {\nfor _, v in vals {\nc := v\nc.n = 1\n_ = c.n\n}\n})",
        "ch := make(chan stats, 1)\n_ = time.after_func(0, fn() {\nv, ok := <-ch\n_ = ok\nv.n = 1\n_ = v.n\n})",
        "s := stats{n: 0}\n_ = time.after_func(0, fn() {\nok, p := pair_ptr(&s)\n_ = p.n\nok = 2\n_ = ok\n})",
        "s := stats{n: 0}\ncbs := []fn(){s.report, s.incr}\nsub := cbs[:1]\nhttp.handle_func(\"/a\", sub[0])\n_ = s",
        "buckets := []map[string]int{map[string]int{\"k\": 0}}\nmanaged(\"foreign sync\", \"interop\", \"#1\") {\n_ = time.after_func(0, fn() {\nfor _, b in buckets { b[\"k\"] = 1 }\n})\n}",
    ] {
        assert_accepts(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}

#[test]
fn channel_alias_composite_literal_and_shadowed_make_target_reject_e2005() {
    assert_rejects_code(
        "type int_chan = chan int\nfn main() {\nc := int_chan{}\n_ = c\n}",
        "HYB-E2005",
    );
    assert_rejects_code(
        "type int_list = []int\nfn main() {\nint_list := 4\ns := make(int_list, 2)\n_ = int_list\n_ = s\n}",
        "HYB-E2005",
    );
    assert_accepts("type int_chan = chan int\nfn main() {\nc := make(int_chan, 1)\n_ = c\n}");
    assert_accepts("type int_list = []int\nfn main() {\ns := make(int_list, 2)\n_ = s\n}");
}
