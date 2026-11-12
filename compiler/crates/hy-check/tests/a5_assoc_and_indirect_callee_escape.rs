//! A5 Go-escape check-site (`HYB-E3013`) — user-declared associated functions (`Type.fn`) and
//! parameterized indirect / first-class callees invoked inside escaping closures.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

const PRE: &str = r#"
import "net/http"
import "time"

type mut_fn = fn(d map[string]int)
type reg_fn = fn(p string, cb fn())
type ops_alias = ops

struct ops {}
struct safe_ops { tag int }
struct unit { tag int }
struct box_map { d map[string]int }
struct op_box {
    r mut_fn
    w mut_fn
}

fn _use_pkgs() {
    _ = time.second
    http.handle_func("/k", fn() {})
}

fn local_reg(p string, cb fn()) {
    _ = p
    cb()
}

fn top_bump(d map[string]int) { d["top"] = d["top"] + 1 }
fn top_read(d map[string]int) { _ = len(d) }

fn (s safe_ops) bump(d map[string]int, k string) {
    _ = s.tag
    _ = d[k]
}

fn (u unit) bump_map(d map[string]int) {
    _ = u.tag
    d["mval"] = d["mval"] + 1
}

fn (u unit) read_map(d map[string]int) {
    _ = u.tag
    _ = len(d)
}

fn (b box_map) apply(f mut_fn) { f(b.d) }

fn ops.bump(d map[string]int, k string) { d[k] = d[k] + 1 }
fn ops.bump_gen[t](d map[string]t, v t) { d["gen"] = v }
fn ops.inspect(d map[string]int) { _ = len(d) }
fn ops.make_cb(d map[string]int) fn() { return fn() { d["fac"] = d["fac"] + 1 } }
fn ops.make_read_cb(d map[string]int) fn() { return fn() { _ = len(d) } }
fn ops.pick_reg() reg_fn { return http.handle_func }
fn ops.pick_local_reg() reg_fn { return local_reg }
fn ops.run_with(sp reg_fn, d map[string]int) { sp("/x", fn() { d["bg"] = 1 }) }
fn ops.run_both(sp reg_fn, cb fn()) { sp("/x", cb) }

fn make_mutator() mut_fn { return fn(d map[string]int) { d["ret"] = d["ret"] + 1 } }
fn make_reader() mut_fn { return fn(d map[string]int) { _ = len(d) } }
fn make_fresh_local() fn(x int) {
    loc := map[string]int{"k": 0}
    return fn(x int) { loc["k"] = x }
}
fn passthru(f mut_fn) mut_fn { return f }
fn invoke_with(op mut_fn, d map[string]int) { op(d) }
fn register_with(op mut_fn, d map[string]int) {
    _ = time.after_func(0, fn() { op(d) })
}
"#;

fn assert_e3013(body: &str) {
    let src = format!("{PRE}\nfn main() {{\n{body}\n}}\n");
    let r = check_file(&parse_file("t.hy", &src).expect("snippet parses"));
    assert!(
        r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E3013"),
        "expected HYB-E3013, got {:?} for:\n{body}",
        r.diagnostics
    );
}

fn assert_clean(body: &str) {
    let src = format!("{PRE}\nfn main() {{\n{body}\n}}\n");
    let r = check_file(&parse_file("t.hy", &src).expect("snippet parses"));
    assert_eq!(
        (r.verdict, r.diagnostics.is_empty()),
        (Verdict::Accept, true),
        "unexpected {:?} for:\n{body}",
        r.diagnostics
    );
}

#[test]
fn associated_fn_escape_paths_reject_e3013() {
    for body in [
        "m := map[string]int{}\n_ = time.after_func(0, fn() { ops.bump(m, \"hits\") })",
        "m := map[string]int{}\n_ = time.after_func(0, fn() { ops.bump_gen[int](m, 1) })",
        "m := map[string]int{}\n_ = time.after_func(0, fn() { ops_alias.bump(m, \"hits\") })",
        "m := map[string]int{}\n_ = time.after_func(0, ops.make_cb(m))",
        "m := map[string]int{}\ncb := ops.make_cb(m)\n_ = time.after_func(0, cb)",
        "m := map[string]int{}\n(ops.pick_reg())(\"/r\", fn() { m[\"reg\"] = 1 })",
        "m := map[string]int{}\nops.run_with(http.handle_func, m)",
        "m := map[string]int{}\nops.run_both(http.handle_func, fn() { m[\"bg\"] = 1 })",
    ] {
        assert_e3013(body);
    }
}

#[test]
fn parameterized_indirect_callees_in_escaping_closures_reject_e3013() {
    for body in [
        "m := map[string]int{}\nlocal_bump := fn(d map[string]int) { d[\"loc\"] = 1 }\n_ = time.after_func(0, fn() { local_bump(m) })",
        "m := map[string]int{}\nfn_val := top_bump\n_ = time.after_func(0, fn() { fn_val(m) })",
        "m := map[string]int{}\nf := top_read\nif len(m) == 0 { f = top_bump }\n_ = time.after_func(0, fn() { f(m) })",
        "m := map[string]int{}\nu := unit{tag: 1}\nmval := u.bump_map\n_ = time.after_func(0, fn() { mval(m) })",
        "m := map[string]int{}\nb := box_map{d: m}\nmv := b.apply\n_ = time.after_func(0, fn() { mv(top_bump) })",
        "m := map[string]int{}\nbx := op_box{r: top_read, w: top_bump}\n_ = time.after_func(0, fn() { bx.w(m) })",
        "m := map[string]int{}\nxs := []mut_fn{top_read, top_bump}\n_ = time.after_func(0, fn() { xs[1](m) })",
        "m := map[string]int{}\nmp := map[string]mut_fn{\"r\": top_read, \"w\": top_bump}\n_ = time.after_func(0, fn() { mp[\"w\"](m) })",
        "m := map[string]int{}\n_ = time.after_func(0, fn() { (make_mutator())(m) })",
        "m := map[string]int{}\n_ = time.after_func(0, fn() { passthru(top_bump)(m) })",
        "m := map[string]int{}\n_ = time.after_func(0, fn() { invoke_with(top_bump, m) })",
        "m := map[string]int{}\nregister_with(top_bump, m)",
    ] {
        assert_e3013(body);
    }
}

#[test]
fn associated_and_indirect_callee_safe_controls_accept() {
    for body in [
        "m := map[string]int{}\n_ = time.after_func(0, fn() { ops.inspect(m) })",
        "_ = time.after_func(0, fn() {\n    local_m := map[string]int{}\n    ops.bump(local_m, \"k\")\n})",
        "m := map[string]int{}\nops := safe_ops{tag: 1}\n_ = time.after_func(0, fn() { ops.bump(m, \"k\") })",
        "m := map[string]int{}\n_ = time.after_func(0, ops.make_read_cb(m))",
        "m := map[string]int{}\n(ops.pick_local_reg())(\"/r\", fn() { m[\"reg\"] = 1 })",
        "m := map[string]int{}\nops.run_with(local_reg, m)\nops.run_both(local_reg, fn() { m[\"bg\"] = 1 })\nops.run_both(http.handle_func, fn() {})",
        "m := map[string]int{}\nfn_val := top_read\n_ = time.after_func(0, fn() { fn_val(m) })",
        "m := map[string]int{}\nf := top_bump\nf = top_read\n_ = time.after_func(0, fn() { f(m) })",
        "m := map[string]int{}\nu := unit{tag: 1}\nmval := u.read_map\n_ = time.after_func(0, fn() { mval(m) })",
        "m := map[string]int{}\nb := box_map{d: m}\nmv := b.apply\n_ = time.after_func(0, fn() { mv(top_read) })",
        "m := map[string]int{}\nbx := op_box{r: top_read, w: top_bump}\nxs := []mut_fn{top_read, top_bump}\nmp := map[string]mut_fn{\"r\": top_read, \"w\": top_bump}\n_ = time.after_func(0, fn() {\n    bx.r(m)\n    xs[0](m)\n    mp[\"r\"](m)\n    (make_reader())(m)\n    passthru(top_read)(m)\n    invoke_with(top_read, m)\n    (make_fresh_local())(1)\n})",
        "local_bump := fn(d map[string]int) { d[\"loc\"] = 1 }\n_ = time.after_func(0, fn() {\n    fresh := map[string]int{}\n    local_bump(fresh)\n})",
        "m := map[string]int{}\nregister_with(top_read, m)",
        "m := map[string]int{}\nfn_val := top_bump\nmanaged(\"timer sync\", \"interop\", \"#1\") {\n    _ = time.after_func(0, fn() {\n        ops.bump(m, \"hits\")\n        fn_val(m)\n    })\n}",
    ] {
        assert_clean(body);
    }
}
