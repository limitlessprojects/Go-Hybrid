use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("unexpected parse errors");
    let checked = check_file(&file);
    checked.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn local_closure_storing_arg_into_captured_container_rejects_e3003_and_e3006() {
    let e3003_src = r#"
fn worker(xs []map[string]int) {
    _ = len(xs)
}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    m := map[string]int{"a": 1}
    put := fn(src map[string]int) {
        xs[0] = src
    }
    put(m)
    task worker(move xs)
    m["b"] = 2
    return 0
}
"#;
    let got = codes(e3003_src);
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when local closure stores argument into captured slice that is moved, got: {:?}",
        got
    );

    let e3006_src = r#"
const shared = map[string]int{"a": 1}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    put := fn(src map[string]int) {
        xs[0] = src
    }
    put(shared)
    xs[0]["b"] = 2
    return len(xs)
}
"#;
    let got_const = codes(e3006_src);
    assert!(
        got_const.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when local closure stores const into captured slice and mutates it, got: {:?}",
        got_const
    );
}

#[test]
fn local_closure_storing_captured_composite_into_out_param_rejects_e3003_e3004_e3006() {
    let e3003_src = r#"
fn worker(xs []map[string]int) {
    _ = len(xs)
}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    m := map[string]int{"a": 1}
    extract := fn(dst []map[string]int) {
        dst[0] = m
    }
    extract(xs)
    task worker(move xs)
    m["b"] = 2
    return 0
}
"#;
    let got_3003 = codes(e3003_src);
    assert!(
        got_3003.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when local closure stores captured map into out-param slice, got: {:?}",
        got_3003
    );

    let e3004_src = r#"
fn worker(xs []map[string]int) {
    _ = len(xs)
}

fn leak(p map[string]int) int {
    xs := []map[string]int{map[string]int{"x": 0}}
    extract := fn(dst []map[string]int) {
        dst[0] = p
    }
    extract(xs)
    task worker(move xs)
    return 0
}
"#;
    let got_3004 = codes(e3004_src);
    assert!(
        got_3004.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 when local closure stores parameter into out-param slice that is moved, got: {:?}",
        got_3004
    );

    let e3006_src = r#"
const shared = map[string]int{"a": 1}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    extract := fn(dst []map[string]int) {
        dst[0] = shared
    }
    extract(xs)
    xs[0]["b"] = 2
    return len(xs)
}
"#;
    let got_3006 = codes(e3006_src);
    assert!(
        got_3006.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when local closure stores captured const into out-param slice, got: {:?}",
        got_3006
    );
}

#[test]
fn local_closure_and_iife_two_param_store_rejects_e3003_and_e3004() {
    let closure_src = r#"
fn worker(xs []map[string]int) {
    _ = len(xs)
}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    m := map[string]int{"a": 1}
    link := fn(dst []map[string]int, src map[string]int) {
        dst[0] = src
    }
    link(xs, m)
    task worker(move xs)
    m["b"] = 2
    return 0
}
"#;
    let got_closure = codes(closure_src);
    assert!(
        got_closure.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when two-param local closure links container and element, got: {:?}",
        got_closure
    );

    let iife_src = r#"
fn worker(xs []map[string]int) {
    _ = len(xs)
}

fn leak(p map[string]int) int {
    xs := []map[string]int{map[string]int{"x": 0}}
    (fn(dst []map[string]int, src map[string]int) {
        dst[0] = src
    })(xs, p)
    task worker(move xs)
    return 0
}
"#;
    let got_iife = codes(iife_src);
    assert!(
        got_iife.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 when inline FnLit call links out-param slice and parameter, got: {:?}",
        got_iife
    );
}

#[test]
fn top_level_fn_and_method_storing_file_const_into_param_rejects_e3006() {
    let fn_src = r#"
const shared = map[string]int{"a": 1}

fn load_const(dst []map[string]int) {
    dst[0] = shared
}

fn run() int {
    xs := []map[string]int{map[string]int{"x": 0}}
    load_const(xs)
    xs[0]["b"] = 2
    return len(xs)
}
"#;
    let got_fn = codes(fn_src);
    assert!(
        got_fn.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when top-level fn stores file const into out-param slice, got: {:?}",
        got_fn
    );

    let method_src = r#"
const shared = map[string]int{"a": 1}

struct box {
    items []map[string]int
}

fn (b *box) load() {
    b.items[0] = shared
}

fn run() int {
    b := box{items: []map[string]int{map[string]int{"x": 0}}}
    b.load()
    b.items[0]["b"] = 2
    return len(b.items)
}
"#;
    let got_method = codes(method_src);
    assert!(
        got_method.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when method stores file const into receiver field, got: {:?}",
        got_method
    );
}

#[test]
fn const_param_helper_mutating_via_match_arm_or_branch_rebind_rejects_e3006() {
    let match_src = r#"
const shared = map[string]int{"a": 1}

fn mutate_in_match(opt option[map[string]int]) {
    match opt {
        some(m) => {
            m["b"] = 2
        }
        none => {}
    }
}

fn run() int {
    mutate_in_match(some(shared))
    return len(shared)
}
"#;
    let got_match = codes(match_src);
    assert!(
        got_match.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper mutates const composite through match arm payload binding, got: {:?}",
        got_match
    );

    let branch_src = r#"
const shared = map[string]int{"a": 1}

fn mutate_after_branch_rebind(m map[string]int, flag bool) {
    alias := map[string]int{"x": 0}
    if flag {
        alias = m
    }
    alias["b"] = 2
}

fn run() int {
    mutate_after_branch_rebind(shared, true)
    return len(shared)
}
"#;
    let got_branch = codes(branch_src);
    assert!(
        got_branch.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper rebinds local to const param inside if branch and mutates after branch, got: {:?}",
        got_branch
    );
}

#[test]
fn const_param_helper_mutating_via_stmt_or_defer_iife_rejects_e3006() {
    let iife_src = r#"
const shared = map[string]int{"a": 1}

fn mutate_via_iife(m map[string]int) {
    (fn() {
        m["b"] = 2
    })()
}

fn run() int {
    mutate_via_iife(shared)
    return len(shared)
}
"#;
    let got_iife = codes(iife_src);
    assert!(
        got_iife.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper mutates const param inside statement IIFE, got: {:?}",
        got_iife
    );

    let defer_src = r#"
const shared = map[string]int{"a": 1}

fn mutate_via_defer_iife(m map[string]int) {
    defer (fn() {
        m["b"] = 2
    })()
}

fn run() int {
    mutate_via_defer_iife(shared)
    return len(shared)
}
"#;
    let got_defer = codes(defer_src);
    assert!(
        got_defer.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper mutates const param inside deferred IIFE, got: {:?}",
        got_defer
    );
}

#[test]
fn const_param_helper_mutating_via_interprocedural_store_or_copy_rejects_e3006() {
    let store_src = r#"
const shared = map[string]int{"a": 1}

fn store_into(dst []map[string]int, src map[string]int) {
    dst[0] = src
}

fn mutate_after_store(m map[string]int) {
    xs := []map[string]int{map[string]int{"x": 0}}
    store_into(xs, m)
    xs[0]["b"] = 2
}

fn run() int {
    mutate_after_store(shared)
    return len(shared)
}
"#;
    let got_store = codes(store_src);
    assert!(
        got_store.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper stores const param into local slice via helper call and mutates it, got: {:?}",
        got_store
    );

    let copy_src = r#"
const shared_rows = []map[string]int{map[string]int{"a": 1}}

fn mutate_after_copy(rows []map[string]int) {
    dst := []map[string]int{map[string]int{"x": 0}}
    copy(dst, rows)
    dst[0]["b"] = 2
}

fn run() int {
    mutate_after_copy(shared_rows)
    return len(shared_rows)
}
"#;
    let got_copy = codes(copy_src);
    assert!(
        got_copy.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper copies const slice of maps into local slice and mutates element, got: {:?}",
        got_copy
    );
}

#[test]
fn positive_controls_read_only_helpers_and_disjoint_closures_accept() {
    let src = r#"
const shared = map[string]int{"a": 1}

fn inspect_match(opt option[map[string]int]) int {
    match opt {
        some(m) => {
            return len(m)
        }
        none => {
            return 0
        }
    }
}

fn inspect_branch(m map[string]int, flag bool) int {
    alias := map[string]int{"x": 0}
    if flag {
        alias = m
    }
    return len(alias)
}

fn worker(xs []map[string]int, done chan int) {
    done <- len(xs)
}

fn run() int {
    done := make(chan int, 1)
    xs := []map[string]int{map[string]int{"x": 0}}
    m := map[string]int{"a": 1}
    read_only := fn(dst []map[string]int, src map[string]int) int {
        return len(dst) + len(src)
    }
    n := read_only(xs, m)
    task worker(move xs, done)
    m["b"] = 2
    _ = <-done
    return n + inspect_match(some(shared)) + inspect_branch(shared, true)
}
"#;
    let got = codes(src);
    assert!(
        got.is_empty(),
        "expected zero diagnostics for read-only helpers and disjoint local closures, got: {:?}",
        got
    );
}
