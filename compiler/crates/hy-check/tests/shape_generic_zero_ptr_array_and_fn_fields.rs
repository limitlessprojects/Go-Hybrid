//! Soundness regression tests for `shape.rs`:
//! 1. `HYB-E4002` (N3): `no_zero_value_reason` substitutes concrete type
//!    arguments on generic struct instantiations (`var h holder[tok]`,
//!    `var h holder[[2]tok]`, `var o outer[holder[tok]]`).
//! 2. Pointer-to-array (`*[N]T`) indexing (`p[i]`) and slicing (`p[:]`):
//!    `shape_of_expr` and `ty_of_expr` preserve element/slice shapes through
//!    `*[N]T` so shared aliases freeze on `move` (`HYB-E3003`) and primitive
//!    elements cross `task` boundaries without false `HYB-E3005`.
//! 3. Calls to struct function fields (`h.cb()`, `h.pair()`), type-aliased
//!    function bindings (`f()`), and indexed/folded function containers
//!    (`fns[0]()`, `fns[i]()`, `h.fns[i]()`) propagate return shapes in
//!    `call_shape`, `call_result_shapes`, and `ty_of_expr`.

use hy_check::{check_source, Verdict};

fn assert_rejects_with(src: &str, code: &str) {
    let report = check_source("test.hy", src).expect("parse ok");
    assert_eq!(
        report.verdict,
        Verdict::Reject,
        "expected Reject with {code}, got Accept:\n{src}"
    );
    assert!(
        report.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got: {:#?}",
        report.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let report = check_source("test.hy", src).expect("parse ok");
    assert_eq!(
        report.verdict,
        Verdict::Accept,
        "expected Accept, got diagnostics: {:#?}",
        report.diagnostics
    );
}

#[test]
fn generic_struct_instantiated_with_no_zero_value_enum_rejected_e4002() {
    assert_rejects_with(
        "enum tok {\n    item(int)\n    empty\n}\nstruct holder[t any] {\n    v t\n}\nfn main() {\n    var h holder[tok]\n    _ = h\n}\n",
        "HYB-E4002",
    );
    assert_rejects_with(
        "enum tok {\n    item(int)\n    empty\n}\nstruct holder[t any] {\n    v t\n}\nstruct outer[u any] {\n    inner u\n}\nfn main() {\n    var o outer[holder[[2]tok]]\n    _ = o\n}\n",
        "HYB-E4002",
    );
    assert_accepts(
        "enum tok {\n    empty\n    item(int)\n}\nstruct holder[t any] {\n    v t\n}\nfn main() {\n    var h holder[tok]\n    _ = h\n}\n",
    );
}

#[test]
fn pointer_to_array_index_and_slice_preserve_shape_for_e3003_and_spawn() {
    assert_rejects_with(
        "fn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn main() {\n    done := make(chan bool, 1)\n    m := map[string]int{\"k\": 0}\n    arr := [1]map[string]int{m}\n    p := &arr\n    x := p[0]\n    alias := x\n    task worker(move x, done)\n    alias[\"k\"] = 2\n    <-done\n}\n",
        "HYB-E3003",
    );
    assert_rejects_with(
        "fn worker(s []int, done chan<- bool) {\n    s[0] = 9\n    done <- true\n}\nfn main() {\n    done := make(chan bool, 1)\n    arr := [2]int{1, 2}\n    p := &arr\n    s := p[:]\n    alias := s\n    task worker(move s, done)\n    alias[0] = 3\n    <-done\n}\n",
        "HYB-E3003",
    );
    assert_accepts(
        "fn worker(n int, done chan<- bool) {\n    _ = n\n    done <- true\n}\nfn main() {\n    done := make(chan bool, 1)\n    arr := [2]int{10, 20}\n    p := &arr\n    x := p[0]\n    task worker(x, done)\n    <-done\n}\n",
    );
}

#[test]
fn struct_fn_field_and_aliased_fn_call_propagate_return_shapes_e3003() {
    assert_rejects_with(
        "struct source {\n    make_map fn() map[string]int\n}\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn run(s source) {\n    done := make(chan bool, 1)\n    m := s.make_map()\n    alias := m\n    task worker(move m, done)\n    alias[\"k\"] = 2\n    <-done\n}\nfn main() {}\n",
        "HYB-E3003",
    );
    assert_rejects_with(
        "struct pair_source {\n    make_pair fn() (map[string]int, int)\n}\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn run(s pair_source) {\n    done := make(chan bool, 1)\n    m, n := s.make_pair()\n    _ = n\n    alias := m\n    task worker(move m, done)\n    alias[\"k\"] = 2\n    <-done\n}\nfn main() {}\n",
        "HYB-E3003",
    );
    assert_rejects_with(
        "type map_maker = fn() map[string]int\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn run(f map_maker) {\n    done := make(chan bool, 1)\n    m := f()\n    alias := m\n    task worker(move m, done)\n    alias[\"k\"] = 2\n    <-done\n}\nfn main() {}\n",
        "HYB-E3003",
    );
}

#[test]
fn indexed_and_folded_fn_containers_propagate_return_shapes_e3003() {
    assert_rejects_with(
        "fn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn run(fns []fn() map[string]int, idx int) {\n    done := make(chan bool, 1)\n    m := fns[idx]()\n    alias := m\n    task worker(move m, done)\n    alias[\"k\"] = 2\n    <-done\n}\nfn main() {}\n",
        "HYB-E3003",
    );
    assert_rejects_with(
        "struct bank {\n    fns []fn() (map[string]int, int)\n}\nfn worker(m map[string]int, done chan<- bool) {\n    m[\"w\"] = 1\n    done <- true\n}\nfn run(b bank, idx int) {\n    done := make(chan bool, 1)\n    m, n := b.fns[idx]()\n    _ = n\n    alias := m\n    task worker(move m, done)\n    alias[\"k\"] = 2\n    <-done\n}\nfn main() {}\n",
        "HYB-E3003",
    );
    assert_accepts(
        "struct int_source {\n    next fn() int\n}\nfn worker(n int, done chan<- bool) {\n    _ = n\n    done <- true\n}\nfn run(s int_source) {\n    done := make(chan bool, 1)\n    n := s.next()\n    task worker(n, done)\n    <-done\n}\nfn main() {}\n",
    );
}
