//! const_write.rs — regression tests for HYB-E3006 (T22 deep immutability
//! on `const`), from soundness issue #37: a file-level `const` slice took
//! element writes and whole-binding rebinds with 0 diagnostics, the emitter
//! lowered it to a package-level `var`, and two spawned tasks raced
//! (`hyb check` OK / `go run -race` DATA RACE). Verdict: `const` is frozen —
//! binding, elements, and fields; reads stay legal; shadowing copies stay
//! writable.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Rejects — every write form through a `const` root
// ---------------------------------------------------------------------------

#[test]
fn file_const_element_write_is_e3006() {
    // Issue #37's exact shape: the index write inside the task body.
    let r = check("const cells = [0]\n\nfn main() {\n    cells[0] = cells[0] + 1\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for the element write through a file-level const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_const_rebind_is_e3006() {
    // The whole-binding form from the issue's acceptance text (`cells = [5]`).
    let r = check("const cells = [0]\n\nfn main() {\n    cells = [5]\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for the whole-binding rebind of a const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn file_const_field_write_is_e3006() {
    // Deep, not shallow: a field store under a const root is frozen too.
    let r = check(
        "struct point {\n    x int\n    y int\n}\n\nconst origin = point{x: 0, y: 0}\n\nfn main() {\n    origin.x = 1\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for the field write through a const root, got {:?}",
        cs
    );
}

#[test]
fn fn_local_const_rebind_is_e3006() {
    // Local const: `Binding::immutable` is set iff LocalKind::Const.
    let r = check("fn main() {\n    const n = 5\n    n = 6\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for rebinding a fn-local const, got {:?}",
        cs
    );
}

#[test]
fn fn_local_const_element_write_is_e3006() {
    let r = check("fn main() {\n    const acc = [0]\n    acc[0] = 1\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for an element write through a fn-local const, got {:?}",
        cs
    );
}

#[test]
fn compound_assign_through_const_is_e3006() {
    // `+=` is a write: the compound op must not slip past the root check.
    let r = check("const total = 0\n\nfn main() {\n    total += 1\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for `+=` through a const, got {:?}",
        cs
    );
}

// ---------------------------------------------------------------------------
// Accepts — reads, shadowing copies, and plain mutable bindings
// ---------------------------------------------------------------------------

#[test]
fn reads_of_const_stay_legal() {
    // Deep immutability bans the write, not the value (E3006 must not fire).
    let r = check(
        "const budget = 3\n\nfn work(x int) int {\n    return x + budget\n}\n\nfn main() {\n    y := work(1)\n    _ = y\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "reading a const must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn shadowing_local_copy_of_const_name_stays_writable() {
    // lookup_binding is locals-first: a `let`/`var` copy that shadows the
    // const name is a fresh mutable binding — the write goes to the copy.
    let r = check("const cells = [0]\n\nfn main() {\n    cells := [1]\n    cells[0] = 2\n}\n");
    assert!(
        codes(&r).is_empty(),
        "a shadowing local copy must stay writable, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn mutable_bindings_stay_writable() {
    // The control: `:=` locals and `var` bindings are the legal home for
    // updating state — E3006 fires only on `const` roots.
    let r = check(
        "fn main() {\n    cells := [0]\n    cells[0] = cells[0] + 1\n    var n int\n    n = n + 1\n    _ = cells\n    _ = n\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "writes through short-declared/var bindings must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn let_interior_mutation_stays_legal() {
    // T22: `let` is SHALLOW immutability — the binding cannot be rebound,
    // but contents stay mutable, so an element write is not E3006's business.
    let r = check("fn main() {\n    let m = map[string]int{}\n    m[\"k\"] = 1\n    _ = m\n}\n");
    assert!(
        codes(&r).is_empty(),
        "interior mutation through a `let` is shallow-legal, got {:?}",
        codes(&r)
    );
}

// ---------------------------------------------------------------------------
// #123 — the paths the first E3006 fix did not walk: closure bodies (the
// A5 perimeter), copies of a const, and delete(). On main every form below
// was ACCEPT with 0 diagnostics; the dual-closure writer program raced at
// runtime under `go run -race`.
// ---------------------------------------------------------------------------

#[test]
fn closure_element_write_is_e3006() {
    // flatten/uses/calls never descend into an FnLit, so process_assign
    // never saw this write — the fn-end FnLit scan catches it now.
    let r = check("const cells = [0]\n\nfn main() {\n    f := fn() {\n        cells[0] = 1\n    }\n    f()\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for an element write inside a closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_rebind_is_e3006() {
    let r = check("const cells = [0]\n\nfn main() {\n    f := fn() {\n        cells = [5]\n    }\n    f()\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for a whole-binding rebind inside a closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn fn_local_const_capture_write_is_e3006() {
    // the completed fn env still holds the fn-local const at scan time.
    let r = check("fn main() {\n    const n = [1]\n    f := fn() {\n        n[0] = 9\n    }\n    f()\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for a captured fn-local const write, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn const_copy_write_is_e3006() {
    // `alias := cells` shares the backing storage — const_write_root follows
    // the alias parent / may-share peers to the immutable root.
    let r = check("const cells = [0]\n\nfn main() {\n    alias := cells\n    alias[0] = 1\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for an element write through a copy of a const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn let_copy_of_const_write_is_e3006() {
    // the `let` flavour of the same copy (deep immutability follows the
    // data, not the binding keyword).
    let r = check("const cells = [0]\n\nfn main() {\n    let copy = cells\n    copy[0] = 1\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for an element write through a `let` copy of a const, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn delete_on_const_is_e3006() {
    // delete() mutates the collection — it never routed through E3006.
    let r = check(
        "const tags = map[string]int{\"a\": 1}\n\nfn main() {\n    delete(tags, \"a\")\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for delete() on a const map, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn delete_on_const_inside_closure_is_e3006() {
    let r = check(
        "const tags = map[string]int{\"a\": 1}\n\nfn main() {\n    f := fn() {\n        delete(tags, \"a\")\n    }\n    f()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for delete() on a const map inside a closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn closure_local_shadowing_const_stays_writable() {
    // the scan's declared-set: a local defined INSIDE the closure shadows
    // the file-level const name — the write goes to the fresh binding.
    let r = check(
        "const cells = [0]\n\nfn main() {\n    f := fn() {\n        cells := [9]\n        cells[0] = 1\n        _ = cells\n    }\n    f()\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "a closure-local shadow of the const name must stay writable, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn closure_param_shadowing_const_stays_writable() {
    let r = check(
        "const cells = [0]\n\nfn main() {\n    f := fn(cells []int) {\n        cells[0] = 1\n    }\n    f([0])\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "a closure param shadow of the const name must stay writable, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn closure_write_to_plain_local_stays_legal() {
    // the control: closure bodies still accept writes to mutable locals.
    let r = check(
        "fn main() {\n    f := fn() {\n        m := map[string]int{}\n        m[\"k\"] = 1\n        _ = m\n    }\n    f()\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "closure writes to plain mutable locals must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// #188 — composite `const` boundary sendability (A1c) + interprocedural
// parameter mutation (HYB-E3006)
// ---------------------------------------------------------------------------

#[test]
fn const_map_two_tasks_spawn_rejects_e3001() {
    let r = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
             for i in 0..10 {\n\
                 m[\"k\"] = i\n\
             }\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done1 := make(chan bool, 1)\n\
             done2 := make(chan bool, 1)\n\
             task worker(shared_map, done1)\n\
             task worker(shared_map, done2)\n\
             <-done1\n\
             <-done2\n\
         }\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when spawning tasks with a const map (#188), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn const_map_channel_send_and_interproc_mutate_reject() {
    let r_send = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn main() {\n\
             ch := make(chan map[string]int, 1)\n\
             ch <- shared_map\n\
         }\n",
    );
    let cs_send = codes(&r_send);
    assert!(
        cs_send.iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 when sending a const map over a channel (#188), got {:?}",
        cs_send
    );
    assert_eq!(r_send.verdict, Verdict::Reject);

    let r_call = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn mutate(m map[string]int) {\n\
             m[\"k\"] = 1\n\
         }\n\n\
         fn main() {\n\
             mutate(shared_map)\n\
         }\n",
    );
    let cs_call = codes(&r_call);
    assert!(
        cs_call.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for interprocedural mutation of a const map (#188), got {:?}",
        cs_call
    );
    assert_eq!(r_call.verdict, Verdict::Reject);
}

#[test]
fn fn_local_const_map_and_const_slice_reject_at_boundary_and_interproc() {
    let r_local = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
             m[\"k\"] = 1\n\
             done <- true\n\
         }\n\n\
         fn mutate(m map[string]int) {\n\
             m[\"k\"] = 2\n\
         }\n\n\
         fn main() {\n\
             const local_map = map[string]int{\"k\": 0}\n\
             done := make(chan bool, 1)\n\
             task worker(local_map, done)\n\
             mutate(local_map)\n\
             <-done\n\
         }\n",
    );
    let cs_local = codes(&r_local);
    assert!(
        cs_local.iter().any(|c| c == "HYB-E3001") && cs_local.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3001 and HYB-E3006 for fn-local const map (#188), got {:?}",
        cs_local
    );

    let r_slice = check(
        "const shared_slice = [0]\n\n\
         fn worker(s []int, done chan<- bool) {\n\
             s[0] = 1\n\
             done <- true\n\
         }\n\n\
         fn mutate(s []int) {\n\
             s[0] = 2\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(shared_slice, done)\n\
             mutate(shared_slice)\n\
             <-done\n\
         }\n",
    );
    let cs_slice = codes(&r_slice);
    assert!(
        cs_slice.iter().any(|c| c == "HYB-E3001") && cs_slice.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3001 and HYB-E3006 for const slice (#188), got {:?}",
        cs_slice
    );
}

#[test]
fn interproc_two_hop_and_copy_on_const_reject_while_readonly_helper_accepts() {
    let r_two_hop = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn inner(m map[string]int) {\n\
             m[\"k\"] = 1\n\
         }\n\n\
         fn outer(m map[string]int) {\n\
             alias := m\n\
             inner(alias)\n\
         }\n\n\
         fn main() {\n\
             outer(shared_map)\n\
         }\n",
    );
    assert!(
        codes(&r_two_hop).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for two-hop interprocedural const mutation, got {:?}",
        codes(&r_two_hop)
    );

    let r_copy = check(
        "const cells = [0]\n\n\
         fn main() {\n\
             src := [1]\n\
             copy(cells, src)\n\
         }\n",
    );
    assert!(
        codes(&r_copy).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for copy() into a const slice, got {:?}",
        codes(&r_copy)
    );

    let r_readonly = check(
        "const shared_map = map[string]int{\"k\": 0}\n\n\
         fn count(m map[string]int) int {\n\
             return len(m)\n\
         }\n\n\
         fn main() {\n\
             n := count(shared_map)\n\
             _ = n\n\
         }\n",
    );
    assert!(
        codes(&r_readonly).is_empty(),
        "read-only helper on a const map must stay accepted, got {:?}",
        codes(&r_readonly)
    );
    assert_eq!(r_readonly.verdict, Verdict::Accept);
}

#[test]
fn const_struct_holding_closure_or_foreign_opaque_rejects_at_boundary() {
    let r_closure_struct = check(
        "struct job {\n\
             f fn()\n\
         }\n\n\
         fn worker(j job, done chan<- bool) {\n\
             j.f()\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             m := map[string]int{}\n\
             const j = job{f: fn() { m[\"k\"] = 1 }}\n\
             done := make(chan bool, 1)\n\
             task worker(j, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_closure_struct).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for const struct holding closure at task boundary, got {:?}",
        codes(&r_closure_struct)
    );
    assert_eq!(r_closure_struct.verdict, Verdict::Reject);

    let r_closure_field = check(
        "struct job {\n\
             f fn()\n\
         }\n\n\
         fn worker(cb fn(), done chan<- bool) {\n\
             cb()\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             m := map[string]int{}\n\
             const j = job{f: fn() { m[\"k\"] = 1 }}\n\
             done := make(chan bool, 1)\n\
             task worker(j.f, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_closure_field).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for closure field projection of const struct, got {:?}",
        codes(&r_closure_field)
    );
    assert_eq!(r_closure_field.verdict, Verdict::Reject);

    // Pure value-copy const struct still crosses cleanly as Class 4.
    let r_value_struct = check(
        "struct pair {\n\
             a int\n\
             b string\n\
         }\n\n\
         const p = pair{a: 1, b: \"ok\"}\n\n\
         fn worker(x pair, done chan<- bool) {\n\
             _ = x\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task worker(p, done)\n\
             <-done\n\
         }\n",
    );
    assert_eq!(
        r_value_struct.verdict,
        Verdict::Accept,
        "{:?}",
        codes(&r_value_struct)
    );
}

#[test]
fn move_of_make_map_and_type_aliased_closure_containers_reject_e3001() {
    let r_make_map = check(
        "fn worker(v map[string]fn(), done chan<- bool) {\n\
             v[\"a\"]()\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             m := map[string]int{}\n\
             v := make(map[string]fn(){})\n\
             v[\"a\"] = fn() { m[\"k\"] = 1 }\n\
             done := make(chan bool, 1)\n\
             task worker(move v, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_make_map).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving make(map[string]fn(){{}}), got {:?}",
        codes(&r_make_map)
    );
    assert_eq!(r_make_map.verdict, Verdict::Reject);

    let r_alias_field = check(
        "type cb = fn()\n\
         struct job {\n\
             f cb\n\
         }\n\n\
         fn worker(j job, done chan<- bool) {\n\
             j.f()\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             m := map[string]int{}\n\
             j := job{f: fn() { m[\"k\"] = 1 }}\n\
             done := make(chan bool, 1)\n\
             task worker(move j, done)\n\
             m[\"k\"] = 2\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_alias_field).iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when moving struct with type-aliased closure field, got {:?}",
        codes(&r_alias_field)
    );
    assert_eq!(r_alias_field.verdict, Verdict::Reject);
}

#[test]
fn closure_interproc_and_method_const_mutations_reject_e3006() {
    let r_closure_call = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn touch(m map[string]int, v int) {\n\
             m[\"a\"] = v\n\
         }\n\n\
         fn main() {\n\
             f := fn() {\n\
                 touch(shared, 2)\n\
             }\n\
             f()\n\
         }\n",
    );
    assert!(
        codes(&r_closure_call).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for interprocedural const mutation inside closure, got {:?}",
        codes(&r_closure_call)
    );
    assert_eq!(r_closure_call.verdict, Verdict::Reject);

    let r_method_recv = check(
        "struct box {\n\
             m map[string]int\n\
         }\n\n\
         const shared_box = box{m: map[string]int{\"a\": 1}}\n\n\
         fn (b box) mutate(v int) {\n\
             b.m[\"a\"] = v\n\
         }\n\n\
         fn main() {\n\
             shared_box.mutate(2)\n\
         }\n",
    );
    assert!(
        codes(&r_method_recv).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for mutating method call on const struct, got {:?}",
        codes(&r_method_recv)
    );
    assert_eq!(r_method_recv.verdict, Verdict::Reject);

    let r_method_arg = check(
        "struct writer {\n\
             tag int\n\
         }\n\n\
         const shared = map[string]int{\"a\": 1}\n\n\
         fn (w writer) put(m map[string]int, v int) {\n\
             _ = w\n\
             m[\"a\"] = v\n\
         }\n\n\
         fn main() {\n\
             w := writer{tag: 1}\n\
             w.put(shared, 2)\n\
         }\n",
    );
    assert!(
        codes(&r_method_arg).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for passing const map to mutating method, got {:?}",
        codes(&r_method_arg)
    );
    assert_eq!(r_method_arg.verdict, Verdict::Reject);

    // Negative control: read-only method on const struct stays accepted.
    let r_method_ro = check(
        "struct box {\n\
             m map[string]int\n\
         }\n\n\
         const shared_box = box{m: map[string]int{\"a\": 1}}\n\n\
         fn (b box) size() int {\n\
             return len(b.m)\n\
         }\n\n\
         fn main() {\n\
             n := shared_box.size()\n\
             _ = n\n\
         }\n",
    );
    assert_eq!(
        r_method_ro.verdict,
        Verdict::Accept,
        "{:?}",
        codes(&r_method_ro)
    );
}

