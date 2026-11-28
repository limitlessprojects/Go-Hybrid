//! A5 Go-escape check-site (`HYB-E3013`) — user-declared associated functions
//! (`fn Type.fn(...)`) and parameterized indirect callees must not hide a
//! closure that mutates a captured shared binding (#4381).
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_e3013(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3013"),
        "expected HYB-E3013, got {:?} for:\n{src}",
        r.diagnostics
    );
}

fn assert_clean(src: &str) {
    let r = check_file(&parse_file("t.hy", src).expect("snippet parses"));
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "unexpected {:?}: {:?} for:\n{src}",
        r.verdict,
        r.diagnostics
    );
}

const PRE: &str = "import \"net/http\"\nimport \"time\"\ntype mut_fn = fn(d map[string]int)\ntype reg_fn = fn(p string, cb fn(w http.response_writer, r *http.request))\ntype ops_alias = ops\nstruct ops {}\nstruct safe_ops { tag int }\nstruct unit { tag int }\nstruct op_box {\n    r mut_fn\n    w mut_fn\n}\nfn (s safe_ops) bump(d map[string]int, k string) {\n    _ = s.tag\n    _ = d[k]\n}\nfn top_bump(d map[string]int) { d[\"top\"] = d[\"top\"] + 1 }\nfn top_read(d map[string]int) { _ = len(d) }\nfn (u unit) bump_map(d map[string]int) {\n    _ = u.tag\n    d[\"mval\"] = d[\"mval\"] + 1\n}\nfn ops.bump(d map[string]int, k string) { d[k] = d[k] + 1 }\nfn ops.bump_gen[t](d map[string]t, v t) { d[\"gen\"] = v }\nfn ops.make_cb(d map[string]int) fn() { return fn() { d[\"factory\"] = d[\"factory\"] + 1 } }\nfn ops.pick_reg() reg_fn { return http.handle_func }\nfn ops.run_with(sp reg_fn, d map[string]int) {\n    sp(\"/assoc\", fn(w http.response_writer, r *http.request) {\n        _, _ = w, r\n        d[\"paired\"] = 1\n    })\n}\nfn make_mutator() mut_fn { return fn(d map[string]int) { d[\"ret\"] = d[\"ret\"] + 1 } }\nfn invoke_with(op mut_fn, d map[string]int) { op(d) }";

#[test]
fn issue_4381_reproducer_1_associated_fn_crash() {
    assert_e3013(
        "import \"fmt\"\nimport \"time\"\nstruct ops {}\nfn ops.bump(m map[string]int, k string) {\n    m[k] = m[k] + 1\n}\nfn main() {\n    m := map[string]int{}\n    start := make(chan bool)\n    done := make(chan bool, 4)\n    for w in 0..4 {\n        _ = time.after_func(0, fn() {\n            _ = <-start\n            for i in 0..50000 {\n                ops.bump(m, \"hits\")\n                _ = i\n            }\n            done <- true\n        })\n        _ = w\n    }\n    close(start)\n    for w in 0..4 {\n        _ = <-done\n        _ = w\n    }\n    fmt.println(\"{len(m)}\")\n}\n",
    );
}

#[test]
fn issue_4381_reproducer_2_associated_functions_reject() {
    for body in [
        "_ = time.after_func(0, fn() { ops.bump(m, \"hits\") })",
        "_ = time.after_func(0, fn() { ops.bump_gen[int](m, 1) })",
        "_ = time.after_func(0, fn() { ops_alias.bump(m, \"hits\") })",
        "_ = time.after_func(0, ops.make_cb(m))",
        "(ops.pick_reg())(\"/r\", fn(w http.response_writer, r *http.request) {\n            _, _ = w, r\n            m[\"reg\"] = 1\n        })",
        "ops.run_with(http.handle_func, m)",
    ] {
        assert_e3013(&format!("{PRE}\nfn main() {{\n    m := map[string]int{{}}\n    {body}\n}}\n"));
    }
}

#[test]
fn issue_4381_reproducer_2_indirect_callees_reject() {
    for body in [
        "local_bump := fn(d map[string]int) { d[\"local\"] = d[\"local\"] + 1 }\n_ = time.after_func(0, fn() { local_bump(m) })",
        "fn_val := top_bump\n_ = time.after_func(0, fn() { fn_val(m) })",
        "u := unit{tag: 1}\nmval := u.bump_map\n_ = time.after_func(0, fn() { mval(m) })",
        "bx := op_box{r: top_read, w: top_bump}\n_ = time.after_func(0, fn() { bx.w(m) })",
        "xs := []mut_fn{top_read, top_bump}\n_ = time.after_func(0, fn() { xs[1](m) })",
        "_ = time.after_func(0, fn() { (make_mutator())(m) })",
        "_ = time.after_func(0, fn() { invoke_with(top_bump, m) })",
    ] {
        assert_e3013(&format!("{PRE}\nfn main() {{\n    m := map[string]int{{}}\n    {body}\n}}\n"));
    }
}

#[test]
fn issue_4381_accept_controls() {
    // Pure foreign functions and read-only callees stay accepted.
    for body in [
        "_ = time.after_func(0, fn() { _ = len(m) })",
        "http.handle_func(\"/k\", fn() {})\n_ = time.second\n_ = m",
        "_ = time.after_func(0, fn() { top_read(m) })",
        "ops := safe_ops{tag: 1}\n_ = time.after_func(0, fn() { ops.bump(m, \"k\") })\n_ = ops",
        "local_read := fn(d map[string]int) { _ = len(d) }\n_ = time.after_func(0, fn() { local_read(m) })",
        "bx := op_box{r: top_read, w: top_bump}\n_ = time.after_func(0, fn() { bx.r(m) })",
        "xs := []mut_fn{top_read, top_bump}\n_ = time.after_func(0, fn() { xs[0](m) })",
        "_ = time.after_func(0, fn() {\n    fresh := map[string]int{}\n    ops.bump(fresh, \"local\")\n})",
        "managed(\"foreign sync\", \"interop\", \"#4381\") { _ = time.after_func(0, fn() { ops.bump(m, \"hits\") }) }",
    ] {
        assert_clean(&format!("{PRE}\nfn main() {{\n    m := map[string]int{{}}\n    {body}\n    _ = m\n}}\n"));
    }
}
