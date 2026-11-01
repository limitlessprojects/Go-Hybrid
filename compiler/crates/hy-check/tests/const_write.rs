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

#[test]
fn read_const_followed_by_blank_assign_accepted_issue_460() {
    // Issue #460: assigning to the blank identifier `_` must not link `_` to a
    // const's share-group or trigger HYB-E3006 on subsequent assignments to `_`.
    let r = check(
        "const shared = map[string]int{\"k\": 1}\n\n\
         fn main() {\n\
             _ = shared[\"k\"]\n\
             xs := [1, 2]\n\
             _ = xs[0]\n\
         }\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for read of const followed by blank index assign (#460), got {:?}",
        codes(&r)
    );
}

#[test]
fn parenthesized_const_reads_to_blank_ident_accepted_issue_460() {
    // Issue #460 minimal reproducer: parenthesized const composite reads to `_`.
    let r = check(
        "const shared_paren = (map[string]int{\"k\": 1})\n\n\
         fn main() {\n\
             const local_paren = ([1, 2])\n\
             _ = shared_paren[\"k\"]\n\
             _ = local_paren[0]\n\
         }\n",
    );
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for parenthesized const reads (#460), got {:?}",
        codes(&r)
    );
}

#[test]
fn clear_on_const_map_slice_field_alias_or_closure_rejects_e3006() {
    let r_file_map = check(
        "const limits = map[string]int{\"free\": 10, \"pro\": 100}\n\n\
         fn reset(done chan<- bool) {\n\
             clear(limits)\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 1)\n\
             task reset(done)\n\
             _ = limits[\"pro\"]\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_file_map).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for clear(limits) on file-level const map, got {:?}",
        codes(&r_file_map)
    );
    assert_eq!(r_file_map.verdict, Verdict::Reject);

    let r_local_slice = check(
        "fn main() {\n\
             const cells = [1, 2, 3]\n\
             clear(cells)\n\
         }\n",
    );
    assert!(
        codes(&r_local_slice).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for clear(cells) on local const slice, got {:?}",
        codes(&r_local_slice)
    );
    assert_eq!(r_local_slice.verdict, Verdict::Reject);

    let r_closure = check(
        "const cfg = map[string]int{\"a\": 1}\n\n\
         fn main() {\n\
             f := fn() {\n\
                 clear(cfg)\n\
             }\n\
             f()\n\
         }\n",
    );
    assert!(
        codes(&r_closure).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for clear(cfg) inside closure body, got {:?}",
        codes(&r_closure)
    );
    assert_eq!(r_closure.verdict, Verdict::Reject);

    let r_alias_and_field = check(
        "struct holder {\n\
             m map[string]int\n\
         }\n\n\
         const h = holder{m: map[string]int{\"a\": 1}}\n\n\
         fn main() {\n\
             alias := h.m\n\
             clear(alias)\n\
             clear(h.m)\n\
         }\n",
    );
    assert_eq!(
        codes(&r_alias_and_field)
            .iter()
            .filter(|c| *c == "HYB-E3006")
            .count(),
        2,
        "expected 2 HYB-E3006 diagnostics for clear on const alias and field, got {:?}",
        codes(&r_alias_and_field)
    );
    assert_eq!(r_alias_and_field.verdict, Verdict::Reject);

    // Negative control: clear() on a mutable local map/slice remains accepted.
    let r_ok = check(
        "fn main() {\n\
             m := map[string]int{\"a\": 1}\n\
             s := [1, 2, 3]\n\
             clear(m)\n\
             clear(s)\n\
         }\n",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", codes(&r_ok));
}

#[test]
fn inplace_package_sort_on_const_slice_rejects_e3006() {
    let r_sort_task = check(
        "const cells = [3, 1, 2]\n\n\
         fn bump(done chan<- bool) {\n\
             sort.ints(cells)\n\
             done <- true\n\
         }\n\n\
         fn main() {\n\
             done := make(chan bool, 2)\n\
             task bump(done)\n\
             task bump(done)\n\
             <-done\n\
             <-done\n\
         }\n",
    );
    assert!(
        codes(&r_sort_task).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for sort.ints(cells) on const slice, got {:?}",
        codes(&r_sort_task)
    );
    assert_eq!(r_sort_task.verdict, Verdict::Reject);

    let r_sort_closure = check(
        "const names = [\"b\", \"a\"]\n\n\
         fn main() {\n\
             f := fn() {\n\
                 sort.strings(names)\n\
             }\n\
             f()\n\
         }\n",
    );
    assert!(
        codes(&r_sort_closure).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for sort.strings(names) in closure, got {:?}",
        codes(&r_sort_closure)
    );
    assert_eq!(r_sort_closure.verdict, Verdict::Reject);

    // Negative control: read-only sort.search_ints on const slice and sort.ints on mutable slice stay accepted.
    let r_ok = check(
        "const sorted = [1, 2, 3]\n\n\
         fn main() {\n\
             idx := sort.search_ints(sorted, 2)\n\
             local := [3, 1, 2]\n\
             sort.ints(local)\n\
             _ = idx\n\
         }\n",
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", codes(&r_ok));
}

#[test]
fn callee_returned_const_composite_rejects_write_and_move() {
    // 1. Top-level helper returning file-level `const` map directly -> HYB-E3006 on write.
    let r_direct = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn get_shared() map[string]int {\n\
             return shared\n\
         }\n\n\
         fn main() {\n\
             m := get_shared()\n\
             m[\"a\"] = 2\n\
         }\n",
    );
    assert_eq!(r_direct.verdict, Verdict::Reject, "{:?}", codes(&r_direct));
    assert!(
        codes(&r_direct).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for write through callee-returned const map, got {:?}",
        codes(&r_direct)
    );

    // 2. Helper returning file-level `const` via local alias -> HYB-E3006 on delete.
    let r_alias = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn get_shared_alias() map[string]int {\n\
             alias := shared\n\
             return alias\n\
         }\n\n\
         fn main() {\n\
             m := get_shared_alias()\n\
             delete(m, \"a\")\n\
         }\n",
    );
    assert_eq!(r_alias.verdict, Verdict::Reject, "{:?}", codes(&r_alias));
    assert!(
        codes(&r_alias).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for delete through callee-returned const alias, got {:?}",
        codes(&r_alias)
    );

    // 3. Helper returning file-level `const` via named result + bare `return` -> HYB-E3006.
    let r_named = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn get_named() (out map[string]int) {\n\
             out = shared\n\
             return\n\
         }\n\n\
         fn main() {\n\
             m := get_named()\n\
             m[\"b\"] = 3\n\
         }\n",
    );
    assert_eq!(r_named.verdict, Verdict::Reject, "{:?}", codes(&r_named));
    assert!(
        codes(&r_named).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for write through named-return const map, got {:?}",
        codes(&r_named)
    );

    // 4. Moving a callee-returned file-level `const` composite -> HYB-E3004 (O5.1).
    let r_move = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn get_shared() map[string]int {\n\
             return shared\n\
         }\n\n\
         fn worker(m map[string]int) {\n\
             _ = m\n\
         }\n\n\
         fn main() {\n\
             m := get_shared()\n\
             task worker(move m)\n\
         }\n",
    );
    assert_eq!(r_move.verdict, Verdict::Reject, "{:?}", codes(&r_move));
    assert!(
        codes(&r_move).iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving callee-returned const map, got {:?}",
        codes(&r_move)
    );

    // 5. Method and local closure returning file-level `const` -> HYB-E3006.
    let r_closure = check(
        "const shared = map[string]int{\"a\": 1}\n\n\
         fn main() {\n\
             get := fn() map[string]int { return shared }\n\
             m := get()\n\
             m[\"x\"] = 9\n\
         }\n",
    );
    assert_eq!(r_closure.verdict, Verdict::Reject, "{:?}", codes(&r_closure));
    assert!(
        codes(&r_closure).iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for write through closure-returned const map, got {:?}",
        codes(&r_closure)
    );

    // 6. Positive control: helper returning a fresh composite accepts both write and move.
    let r_fresh = check(
        "fn get_fresh() map[string]int {\n\
             return map[string]int{\"a\": 1}\n\
         }\n\n\
         fn worker(m map[string]int) {\n\
             _ = m\n\
         }\n\n\
         fn main() {\n\
             m := get_fresh()\n\
             m[\"a\"] = 2\n\
             task worker(move m)\n\
         }\n",
    );
    assert_eq!(r_fresh.verdict, Verdict::Accept, "{:?}", codes(&r_fresh));
}

#[test]
fn multi_value_return_intra_callee_sharing_rejects_post_move_and_a3() {
    // 1. Intra-callee local alias `m2 := m; return m, m2` -> HYB-E3003 on post-move use.
    let r_alias = check(
        "fn pair_alias() (map[string]int, map[string]int) {\n\
             m := map[string]int{\"a\": 1}\n\
             m2 := m\n\
             return m, m2\n\
         }\n\n\
         fn worker(m map[string]int) { m[\"k\"] = 1 }\n\n\
         fn main() {\n\
             a, b := pair_alias()\n\
             task worker(move a)\n\
             b[\"x\"] = 2\n\
         }\n",
    );
    assert_eq!(r_alias.verdict, Verdict::Reject, "{:?}", codes(&r_alias));
    assert!(
        codes(&r_alias).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for intra-callee alias multi-return, got {:?}",
        codes(&r_alias)
    );

    // 2. Intra-callee slice projections `return s[0:1], s[1:2]` -> HYB-E3003.
    let r_slice = check(
        "fn pair_slice() ([]int, []int) {\n\
             s := [1, 2, 3]\n\
             return s[0:1], s[1:2]\n\
         }\n\n\
         fn worker(xs []int) { xs[0] = 9 }\n\n\
         fn main() {\n\
             a, b := pair_slice()\n\
             task worker(move a)\n\
             b[0] = 7\n\
         }\n",
    );
    assert_eq!(r_slice.verdict, Verdict::Reject, "{:?}", codes(&r_slice));
    assert!(
        codes(&r_slice).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for overlapping slice multi-return, got {:?}",
        codes(&r_slice)
    );

    // 3. Composite literal wrapping same storage `return holder{m: m}, m` -> HYB-E3003.
    let r_holder = check(
        "struct holder { m map[string]int }\n\n\
         fn pair_holder() (holder, map[string]int) {\n\
             m := map[string]int{\"a\": 1}\n\
             return holder{m: m}, m\n\
         }\n\n\
         fn worker(h holder) { h.m[\"k\"] = 1 }\n\n\
         fn main() {\n\
             h, m := pair_holder()\n\
             task worker(move h)\n\
             m[\"x\"] = 2\n\
         }\n",
    );
    assert_eq!(r_holder.verdict, Verdict::Reject, "{:?}", codes(&r_holder));
    assert!(
        codes(&r_holder).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for struct-literal + map multi-return, got {:?}",
        codes(&r_holder)
    );

    // 4. Named return parameters with bare `return` (`a = m; b = m; return`) -> HYB-E3003.
    let r_named = check(
        "fn pair_named() (a map[string]int, b map[string]int) {\n\
             m := map[string]int{\"a\": 1}\n\
             a = m\n\
             b = m\n\
             return\n\
         }\n\n\
         fn worker(m map[string]int) { m[\"k\"] = 1 }\n\n\
         fn main() {\n\
             x, y := pair_named()\n\
             task worker(move x)\n\
             y[\"x\"] = 2\n\
         }\n",
    );
    assert_eq!(r_named.verdict, Verdict::Reject, "{:?}", codes(&r_named));
    assert!(
        codes(&r_named).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for named-return bare return sharing, got {:?}",
        codes(&r_named)
    );

    // 5. Local closure and closure-body multi-define -> HYB-E3003.
    let r_closure = check(
        "fn pair_alias() (map[string]int, map[string]int) {\n\
             m := map[string]int{\"a\": 1}\n\
             m2 := m\n\
             return m, m2\n\
         }\n\n\
         fn worker(m map[string]int) { m[\"k\"] = 1 }\n\n\
         fn main() {\n\
             run := fn() {\n\
                 a, b := pair_alias()\n\
                 task worker(move a)\n\
                 b[\"x\"] = 2\n\
             }\n\
             run()\n\
         }\n",
    );
    assert_eq!(r_closure.verdict, Verdict::Reject, "{:?}", codes(&r_closure));
    assert!(
        codes(&r_closure).iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 inside closure body multi-define, got {:?}",
        codes(&r_closure)
    );

    // 6. A3 map-slot loan invalidation across multi-value shared return -> HYB-E3102.
    let r_a3 = check(
        "import \"fmt\"\n\
         struct svc { count int }\n\n\
         fn pair_alias() (map[string]svc, map[string]svc) {\n\
             m := map[string]svc{\"a\": svc{count: 1}}\n\
             m2 := m\n\
             return m, m2\n\
         }\n\n\
         fn main() {\n\
             a, b := pair_alias()\n\
             s := &a[\"a\"]\n\
             b[\"b\"] = svc{count: 2}\n\
             fmt.println(s.count)\n\
         }\n",
    );
    assert_eq!(r_a3.verdict, Verdict::Reject, "{:?}", codes(&r_a3));
    assert!(
        codes(&r_a3).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for A3 insert through shared multi-return peer, got {:?}",
        codes(&r_a3)
    );

    // 7. Positive control: disjoint maps returned from a helper using a scalar seed accept.
    let r_disjoint = check(
        "fn pair_disjoint(seed int) (map[string]int, map[string]int) {\n\
             return map[string]int{\"a\": seed}, map[string]int{\"b\": seed}\n\
         }\n\n\
         fn worker(m map[string]int) { m[\"k\"] = 1 }\n\n\
         fn main() {\n\
             a, b := pair_disjoint(1)\n\
             task worker(move a)\n\
             b[\"x\"] = 2\n\
         }\n",
    );
    assert_eq!(
        r_disjoint.verdict,
        Verdict::Accept,
        "{:?}",
        codes(&r_disjoint)
    );
}

#[test]
fn let_immutability_rejects_all_reassignment_forms_issue_655() {
    // Repro 1: direct reassignment `=`
    let r1 = check(
        "import \"fmt\"\n\nfn main() {\n    let limit = 10\n    limit = 20\n    fmt.println(limit)\n}\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", codes(&r1));
    assert!(codes(&r1).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r1));

    // Repro 2: compound assignment `+=` and increment `++`
    let r2 = check(
        "import \"fmt\"\n\nfn main() {\n    let limit = 10\n    limit += 1\n    limit++\n    fmt.println(limit)\n}\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", codes(&r2));
    assert_eq!(
        codes(&r2).iter().filter(|c| *c == "HYB-E3006").count(),
        2,
        "{:?}",
        codes(&r2)
    );

    // Repro 3: reassignment from inside a closure
    let r3 = check(
        "import \"fmt\"\n\nfn main() {\n    let limit = 10\n    f := fn() {\n        limit = 99\n    }\n    f()\n    fmt.println(limit)\n}\n",
    );
    assert_eq!(r3.verdict, Verdict::Reject, "{:?}", codes(&r3));
    assert!(codes(&r3).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r3));

    // Repro 4: multi-target tuple assignment writing to a `let` binding
    let r4 = check(
        "import \"fmt\"\n\nfn two() (int, int) {\n    return 1, 2\n}\n\nfn main() {\n    let limit = 10\n    other := 0\n    limit, other = two()\n    fmt.println(limit, other)\n}\n",
    );
    assert_eq!(r4.verdict, Verdict::Reject, "{:?}", codes(&r4));
    assert!(codes(&r4).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r4));

    // Controls: reading a `let`, reassigning `var`/`:=`, and mutating elements
    // through a `let` slice binding (shallow binding immutability) all accept.
    for ok_src in [
        "import \"fmt\"\n\nfn main() {\n    let limit = 10\n    fmt.println(limit)\n}\n",
        "import \"fmt\"\n\nfn main() {\n    var count = 10\n    count = 20\n    step := 1\n    step += 2\n    fmt.println(count + step)\n}\n",
        "import \"fmt\"\n\nfn main() {\n    let names = []string{\"a\", \"b\"}\n    names[0] = \"z\"\n    fmt.println(names[0])\n}\n",
    ] {
        let r_ok = check(ok_src);
        assert_eq!(r_ok.verdict, Verdict::Accept, "src:\n{ok_src}\ndiags: {:?}", r_ok.diagnostics);
    }
}

#[test]
fn non_addressable_pointer_receiver_method_call_rejected_issue_656() {
    // Repro 1: map element receiver
    let r1 = check(
        "import \"fmt\"\n\nstruct counter {\n    n int\n}\n\nfn (self *counter) bump() {\n    self.n += 1\n}\n\nfn main() {\n    m := map[string]counter{\"a\": counter{n: 1}}\n    m[\"a\"].bump()\n    fmt.println(m[\"a\"].n)\n}\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", codes(&r1));
    assert!(codes(&r1).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(&r1));

    // Repro 2: function call result receiver
    let r2 = check(
        "struct counter {\n    n int\n}\n\nfn (self *counter) bump() {\n    self.n += 1\n}\n\nfn make_counter() counter {\n    return counter{n: 1}\n}\n\nfn main() {\n    make_counter().bump()\n}\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", codes(&r2));
    assert!(codes(&r2).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(&r2));

    // Repro 3: struct literal receiver
    let r3 = check(
        "struct counter {\n    n int\n}\n\nfn (self *counter) bump() {\n    self.n += 1\n}\n\nfn main() {\n    (counter{n: 1}).bump()\n}\n",
    );
    assert_eq!(r3.verdict, Verdict::Reject, "{:?}", codes(&r3));
    assert!(codes(&r3).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(&r3));

    // Controls: addressable local variable, addressable slice element, and
    // value-receiver method on a map element or call result all accept.
    for ok_src in [
        "import \"fmt\"\n\nstruct counter {\n    n int\n}\n\nfn (self *counter) bump() {\n    self.n += 1\n}\n\nfn main() {\n    c := counter{n: 1}\n    c.bump()\n    s := []counter{counter{n: 1}}\n    s[0].bump()\n    fmt.println(c.n, s[0].n)\n}\n",
        "import \"fmt\"\n\nstruct counter {\n    n int\n}\n\nfn (self counter) get() int {\n    return self.n\n}\n\nfn main() {\n    m := map[string]counter{\"a\": counter{n: 1}}\n    fmt.println(m[\"a\"].get())\n}\n",
    ] {
        let r_ok = check(ok_src);
        assert_eq!(r_ok.verdict, Verdict::Accept, "src:\n{ok_src}\ndiags: {:?}", r_ok.diagnostics);
    }
}

#[test]
fn trailing_dot_float_and_errors_wrap_check_clean_issues_657_659() {
    let r659 = check("import \"fmt\"\n\nfn main() {\n    x := 2.\n    fmt.println(x)\n}\n");
    assert_eq!(r659.verdict, Verdict::Accept, "{:?}", r659.diagnostics);

    let r657 = check(
        "import \"errors\"\nimport \"fmt\"\n\nfn load() error {\n    err := fmt.errorf(\"disk full\")\n    return errors.wrap(err, \"reading config\")\n}\n\nfn main() {\n    fmt.println(load())\n}\n",
    );
    assert_eq!(r657.verdict, Verdict::Accept, "{:?}", r657.diagnostics);
}


#[test]
fn const_write_rejects_indirect_and_aliased_fn_calls_issue_1363() {
    // 1. Slice-indexed function call mutating a const slice
    let r_slice = check(
        r#"const c = []int{1, 2}
fn touch(xs []int) { xs[0] = 9 }
fn main() {
    fs := []fn(xs []int){touch}
    fs[0](c)
}
"#,
    );
    assert_eq!(r_slice.verdict, Verdict::Reject, "{:?}", codes(&r_slice));
    assert!(codes(&r_slice).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r_slice));

    // 2. Map-indexed function call mutating a const slice
    let r_map = check(
        r#"const c = []int{1, 2}
fn touch(xs []int) { xs[0] = 9 }
fn main() {
    fm := map[string]fn(xs []int){"k": touch}
    fm["k"](c)
}
"#,
    );
    assert_eq!(r_map.verdict, Verdict::Reject, "{:?}", codes(&r_map));
    assert!(codes(&r_map).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r_map));

    // 3. Returned function call mutating a const slice
    let r_ret = check(
        r#"const c = []int{1, 2}
fn touch(xs []int) { xs[0] = 9 }
fn pick() fn(xs []int) { return touch }
fn main() {
    pick()(c)
}
"#,
    );
    assert_eq!(r_ret.verdict, Verdict::Reject, "{:?}", codes(&r_ret));
    assert!(codes(&r_ret).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r_ret));

    // 4. Channel-received function call mutating a const slice
    let r_chan = check(
        r#"const c = []int{1, 2}
fn touch(xs []int) { xs[0] = 9 }
fn run(ch <-chan fn(xs []int)) {
    (<-ch)(c)
}
fn main() {}
"#,
    );
    assert_eq!(r_chan.verdict, Verdict::Reject, "{:?}", codes(&r_chan));
    assert!(codes(&r_chan).iter().any(|c| c == "HYB-E3006"), "{:?}", codes(&r_chan));

    // 5. Struct field and type-aliased function taking *T auto-addressing a const struct
    let r_ptr = check(
        r#"struct item { v int }
struct runner { f fn(p *item) }
type mut_fn = fn(p *item)
const c = item{ v: 1 }
fn touch(p *item) { p.v = 2 }
fn main() {
    r := runner{ f: touch }
    r.f(c)
    var f mut_fn = touch
    f(c)
}
"#,
    );
    assert_eq!(r_ptr.verdict, Verdict::Reject, "{:?}", codes(&r_ptr));
    assert_eq!(
        codes(&r_ptr).iter().filter(|c| *c == "HYB-E3006").count(),
        2,
        "{:?}",
        codes(&r_ptr)
    );

    // 6. Control: indirect calls with mutable arguments check cleanly
    let r_ok = check(
        r#"type mut_fn = fn(p *item)
struct item { v int }
struct runner { f fn(p *item) }
fn touch_slice(xs []int) { xs[0] = 9 }
fn touch_item(p *item) { p.v = 2 }
fn pick() fn(xs []int) { return touch_slice }
fn main() {
    xs := []int{1, 2}
    fs := []fn(xs []int){touch_slice}
    fs[0](xs)
    pick()(xs)
    it := item{ v: 1 }
    r := runner{ f: touch_item }
    r.f(it)
    var f mut_fn = touch_item
    f(it)
}
"#,
    );
    assert_eq!(r_ok.verdict, Verdict::Accept, "{:?}", r_ok.diagnostics);
}
