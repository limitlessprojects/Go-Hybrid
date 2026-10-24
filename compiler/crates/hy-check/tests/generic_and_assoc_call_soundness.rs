#[test]
fn generic_and_assoc_calls_propagate_param_stores_and_non_owned_origins() {
    let src = r#"
struct helper {
    tag int
}

fn store_generic[V any](dst map[string]V, src V) {
    dst["k"] = src
}

fn helper.store_assoc(dst map[string]map[string]int, src map[string]int) {
    dst["k"] = src
}

fn worker(owned map[string]int) {
    owned["race"] = 1
}

fn leak_via_explicit_generic(borrowed map[string]int) {
    outer1 := map[string]map[string]int{}
    inner1 := map[string]int{"a": 1}
    store_generic[map[string]int](outer1, inner1)
    task worker(move inner1)
    outer1["k"]["main"] = 2

    outer2 := map[string]map[string]int{}
    store_generic[map[string]int](outer2, borrowed)
    task worker(move outer2["k"])
}

fn leak_via_assoc_fn(borrowed map[string]int) {
    outer1 := map[string]map[string]int{}
    inner1 := map[string]int{"a": 1}
    helper.store_assoc(outer1, inner1)
    task worker(move inner1)
    outer1["k"]["main"] = 2

    outer2 := map[string]map[string]int{}
    helper.store_assoc(outer2, borrowed)
    task worker(move outer2)
}

fn main() {
    m := map[string]int{"init": 0}
    leak_via_explicit_generic(m)
    leak_via_assoc_fn(m)
}
"#;
    let file = hy_syntax::parse_file(src, "test.hy").expect("parse");
    let res = hy_check::check_file(&file);
    assert_eq!(res.verdict, hy_check::Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3004"));
}

#[test]
fn generic_assoc_and_indexed_callees_reject_const_mutations() {
    let src = r#"
const SHARED = map[string]int{"a": 1}

struct helper {
    tag int
}

fn mutate_generic[V any](m map[string]V, v V) {
    m["a"] = v
}

fn helper.mutate_assoc(m map[string]int, v int) {
    m["a"] = v
}

fn helper.leak_const() map[string]int {
    return SHARED
}

fn main() {
    mutate_generic[int](SHARED, 2)
    helper.mutate_assoc(SHARED, 3)
    leaked := helper.leak_const()
    leaked["a"] = 4
    writers := map[string]fn(map[string]int){
        "go": fn(m map[string]int) { m["a"] = 5 },
    }
    k := "go"
    writers[k](SHARED)
    writers["go"](SHARED)
}
"#;
    let file = hy_syntax::parse_file(src, "test.hy").expect("parse");
    let res = hy_check::check_file(&file);
    assert_eq!(res.verdict, hy_check::Verdict::Reject);
    let e3006 = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E3006")
        .count();
    assert!(e3006 >= 5, "expected >= 5 HYB-E3006 diagnostics, got {e3006}");
}

#[test]
fn generic_and_assoc_closure_factories_and_wrapped_calls_enforce_a3_loans() {
    let src = r#"
struct item {
    count int
}

struct wrap {
    m map[string]item
}

struct helper {
    tag int
}

fn make_mutator[V any](m map[string]item, v V) fn() {
    _ = v
    return fn() {
        m["b"] = item{count: 99}
    }
}

fn helper.make_assoc_mutator(m map[string]item) fn() {
    return fn() {
        m["c"] = item{count: 100}
    }
}

fn insert_wrapped[V any](w wrap, v V) {
    _ = v
    w.m["d"] = item{count: 101}
}

fn main() {
    m := map[string]item{"a": item{count: 1}}
    f1 := make_mutator[int](m, 0)
    f2 := helper.make_assoc_mutator(m)

    entry := m["a"]
    f1()
    f2()
    insert_wrapped[int](wrap{m: m}, 0)
    entry.count = 42
}
"#;
    let file = hy_syntax::parse_file(src, "test.hy").expect("parse");
    let res = hy_check::check_file(&file);
    assert_eq!(res.verdict, hy_check::Verdict::Reject);
    let e3102 = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E3102")
        .count();
    assert!(e3102 >= 3, "expected >= 3 HYB-E3102 diagnostics, got {e3102}");
}
