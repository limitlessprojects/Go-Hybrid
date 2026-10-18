//! a3_interproc.rs — regression tests for issue #111: A3 map-slot-loan
//! invalidation events that do not sit lexically in the loaned frame.
//! Before the fix the direct twins rejected HYB-E3102 while the same
//! runtime event through a callee frame, a shallow alias, or a closure
//! body was accepted with 0 diagnostics (and a deleted loaned key silently
//! resurrected at runtime via the loan's store-back).

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

fn assert_e3102(r: &hy_check::CheckResult, what: &str) {
    let cs = codes(r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 ({}) , got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Rejects — the event crosses a frame / alias / closure, window still live
// ---------------------------------------------------------------------------

#[test]
fn helper_insert_in_window_is_e3102() {
    // #111 form 1: the insert runs in bump's frame, inside the window.
    let r = check(
        "struct cell { n int }\n\
         fn bump(m map[string]cell, k string) {\n\
         \x20   m[k] = cell{n: 1}\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   bump(m, \"fresh\")\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert_e3102(&r, "helper insert in window");
}

#[test]
fn helper_delete_in_window_is_e3102() {
    // #111 form 2: delete spelled inside the callee.
    let r = check(
        "struct cell { n int }\n\
         fn drop(m map[string]cell, k string) {\n\
         \x20   delete(m, k)\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}, \"b\": cell{n: 1}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   drop(m, \"b\")\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert_e3102(&r, "helper delete in window");
}

#[test]
fn alias_insert_in_window_is_e3102() {
    // #111 form 3: insert on a may-share peer of the loaned base.
    let r = check(
        "struct cell { n int }\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   alias := m\n\
         \x20   e := &m[\"a\"]\n\
         \x20   alias[\"fresh\"] = cell{n: 1}\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert_e3102(&r, "alias insert in window");
}

#[test]
fn closure_insert_in_window_is_e3102() {
    // #111 form 4: the insert sits in the closure body; the CALL is in
    // the window, so the event counts there.
    let r = check(
        "struct cell { n int }\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   f := fn() {\n\
         \x20       m[\"fresh\"] = cell{n: 1}\n\
         \x20   }\n\
         \x20   f()\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert_e3102(&r, "closure insert in window");
}

#[test]
fn nested_helper_insert_in_window_is_e3102() {
    // The mutation is two frames deep: main -> outer -> inner.
    let r = check(
        "struct cell { n int }\n\
         fn inner(m map[string]cell) {\n\
         \x20   m[\"fresh\"] = cell{n: 1}\n\
         }\n\
         fn outer(m map[string]cell) {\n\
         \x20   inner(m)\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   outer(m)\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert_e3102(&r, "two-frame helper insert in window");
}

// ---------------------------------------------------------------------------
// Accepts — the window is closed, the callee is read-only, or the closure
// never runs while the loan is live (no over-rejection)
// ---------------------------------------------------------------------------

#[test]
fn helper_delete_after_window_accepts() {
    // Corpus 09's legal twin with the delete in a callee: last use of the
    // loan BEFORE the call — the window is already closed (h30).
    let r = check(
        "struct cell { n int }\n\
         fn drop(m map[string]cell, k string) {\n\
         \x20   delete(m, k)\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}, \"b\": cell{n: 1}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   e.n = 9\n\
         \x20   drop(m, \"b\")\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "delete after the loan's last use must stay accepted: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn read_only_helper_in_window_accepts() {
    // The call passes the loaned map but only reads it (len) — no A3
    // event executes, the window survives (fail-closed on unknown callees,
    // pure on the known read-only ones).
    let r = check(
        "struct cell { n int }\n\
         fn count(m map[string]cell) int {\n\
         \x20   return len(m)\n\
         }\n\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   n := count(m)\n\
         \x20   e.n = n\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "a read-only helper inside the window must stay accepted: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn closure_called_after_window_accepts() {
    // The closure body mutates m, but f() runs AFTER the loan's last use.
    let r = check(
        "struct cell { n int }\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   e.n = 9\n\
         \x20   f := fn() {\n\
         \x20       m[\"fresh\"] = cell{n: 1}\n\
         \x20   }\n\
         \x20   f()\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "closure called after the window must stay accepted: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn shadowing_closure_param_is_not_a_mutation_of_the_outer_map() {
    // The closure declares its OWN `m` parameter — its body mutates the
    // fresh binding, not the loaned outer map (mirrors capture_freeze's
    // shadowing precision).
    let r = check(
        "struct cell { n int }\n\
         fn main() {\n\
         \x20   m := map[string]cell{\"a\": cell{n: 0}}\n\
         \x20   e := &m[\"a\"]\n\
         \x20   f := fn(m map[string]cell) {\n\
         \x20       m[\"fresh\"] = cell{n: 1}\n\
         \x20   }\n\
         \x20   f(map[string]cell{})\n\
         \x20   e.n = 9\n\
         }\n",
    );
    assert!(
        codes(&r).is_empty(),
        "a shadowing closure param is not the loaned binding: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn clear_in_slot_loan_window_rejects_e3102() {
    let r_direct = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             p := &m[\"a\"]\n\
             clear(m)\n\
             p.n = 5\n\
         }\n",
    );
    assert_eq!(codes(&r_direct), vec!["HYB-E3102"]);
    assert!(
        r_direct.diagnostics[0]
            .message
            .contains("map clear invalidates the live slot loan `p`"),
        "unexpected message: {}",
        r_direct.diagnostics[0].message
    );
    assert_eq!(r_direct.verdict, Verdict::Reject);

    let r_closure = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             p := &m[\"a\"]\n\
             wipe := fn() {\n\
                 clear(m)\n\
             }\n\
             wipe()\n\
             p.n = 5\n\
         }\n",
    );
    assert_eq!(codes(&r_closure), vec!["HYB-E3102"]);
    assert_eq!(r_closure.verdict, Verdict::Reject);

    let r_helper = check(
        "struct cell { n int }\n\
         fn wipe(m map[string]cell) {\n\
             clear(m)\n\
         }\n\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             p := &m[\"a\"]\n\
             wipe(m)\n\
             p.n = 5\n\
         }\n",
    );
    assert_e3102(&r_helper, "helper clear in window");
}

#[test]
fn map_and_container_rebind_in_slot_loan_window_rejects_e3102() {
    // Direct map rebind `m = m2` while `p := &m["a"]` is live
    let r_map_rebind = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             m2 := map[string]cell{\"a\": cell{n: 2}}\n\
             p := &m[\"a\"]\n\
             p.n = 42\n\
             m = m2\n\
             _ = p.n\n\
         }\n",
    );
    assert_e3102(&r_map_rebind, "direct map rebind in window");

    // Enclosing struct field rebind `h.m = m2` and whole-struct rebind `h = h2`
    let r_field_rebind = check(
        "struct cell { n int }\n\
         struct holder { m map[string]cell }\n\
         fn main() {\n\
             h := holder{m: map[string]cell{\"a\": cell{n: 1}}}\n\
             p := &h.m[\"a\"]\n\
             h.m = map[string]cell{\"a\": cell{n: 2}}\n\
             p.n = 42\n\
         }\n",
    );
    assert_e3102(&r_field_rebind, "struct map-field rebind in window");

    let r_struct_rebind = check(
        "struct cell { n int }\n\
         struct holder { m map[string]cell }\n\
         fn main() {\n\
             h := holder{m: map[string]cell{\"a\": cell{n: 1}}}\n\
             h2 := holder{m: map[string]cell{\"a\": cell{n: 2}}}\n\
             p := &h.m[\"a\"]\n\
             h = h2\n\
             p.n = 42\n\
         }\n",
    );
    assert_e3102(&r_struct_rebind, "enclosing struct rebind in window");

    // Closure rebinding `m = m2` called inside the live window
    let r_closure_rebind = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             m2 := map[string]cell{\"a\": cell{n: 2}}\n\
             p := &m[\"a\"]\n\
             swap := fn() {\n\
                 m = m2\n\
             }\n\
             swap()\n\
             p.n = 42\n\
         }\n",
    );
    assert_e3102(&r_closure_rebind, "closure map rebind in window");

    // Rebinding after the loan's last use stays accepted
    let r_after = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}}\n\
             m2 := map[string]cell{\"a\": cell{n: 2}}\n\
             p := &m[\"a\"]\n\
             p.n = 42\n\
             m = m2\n\
         }\n",
    );
    assert!(codes(&r_after).is_empty(), "{:?}", r_after.diagnostics);
    assert_eq!(r_after.verdict, Verdict::Accept);
}

#[test]
fn user_method_assoc_fn_inline_closure_and_paren_callee_in_window_reject_e3102() {
    // User method mutating receiver's map
    let r_method_recv = check(
        "struct cell { n int }\n\
         struct holder { m map[string]cell }\n\
         fn (h *holder) insert_fresh() {\n\
             h.m[\"fresh\"] = cell{n: 1}\n\
         }\n\
         fn main() {\n\
             h := holder{m: map[string]cell{\"a\": cell{n: 0}}}\n\
             p := &h.m[\"a\"]\n\
             h.insert_fresh()\n\
             p.n = 9\n\
         }\n",
    );
    assert_e3102(&r_method_recv, "user method mutating receiver map in window");

    // User method mutating map argument
    let r_method_arg = check(
        "struct cell { n int }\n\
         struct mutator { tag int }\n\
         fn (x *mutator) put(m map[string]cell) {\n\
             m[\"fresh\"] = cell{n: x.tag}\n\
         }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 0}}\n\
             x := mutator{tag: 7}\n\
             p := &m[\"a\"]\n\
             x.put(m)\n\
             p.n = 9\n\
         }\n",
    );
    assert_e3102(&r_method_arg, "user method mutating map arg in window");

    // Associated function mutating map argument
    let r_assoc = check(
        "struct cell { n int }\n\
         struct holder { m map[string]cell }\n\
         fn holder.put(h *holder) {\n\
             h.m[\"fresh\"] = cell{n: 1}\n\
         }\n\
         fn main() {\n\
             h := holder{m: map[string]cell{\"a\": cell{n: 0}}}\n\
             p := &h.m[\"a\"]\n\
             holder.put(&h)\n\
             p.n = 9\n\
         }\n",
    );
    assert_e3102(&r_assoc, "associated fn mutating map in window");

    // Inline closure call `(fn() { m["fresh"] = ... })()`
    let r_inline_closure = check(
        "struct cell { n int }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 0}}\n\
             p := &m[\"a\"]\n\
             if true {\n\
                 (fn() {\n\
                     m[\"fresh\"] = cell{n: 1}\n\
                 })()\n\
             }\n\
             p.n = 9\n\
         }\n",
    );
    assert_e3102(&r_inline_closure, "inline closure mutating map in window");

    // Parenthesized callee `(bump)(m)`
    let r_paren_callee = check(
        "struct cell { n int }\n\
         fn bump(m map[string]cell) {\n\
             m[\"fresh\"] = cell{n: 1}\n\
         }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 0}}\n\
             p := &m[\"a\"]\n\
             if true {\n\
                 (bump)(m)\n\
             }\n\
             p.n = 9\n\
         }\n",
    );
    assert_e3102(&r_paren_callee, "parenthesized callee mutating map in window");

    // Read-only user method inside window stays accepted
    let r_readonly_method = check(
        "struct cell { n int }\n\
         struct holder { m map[string]cell }\n\
         fn (h *holder) size() int {\n\
             return len(h.m)\n\
         }\n\
         fn main() {\n\
             h := holder{m: map[string]cell{\"a\": cell{n: 0}}}\n\
             p := &h.m[\"a\"]\n\
             k := h.size()\n\
             p.n = k\n\
         }\n",
    );
    assert!(
        codes(&r_readonly_method).is_empty(),
        "{:?}",
        r_readonly_method.diagnostics
    );
    assert_eq!(r_readonly_method.verdict, Verdict::Accept);
}

#[test]
fn plain_rebind_ends_slot_loan_window_and_clears_t24_flag() {
    // Once `p` is rebound to a plain value (`p = cell{n: 2}` or `p, ok = m["b"]`),
    // the slot loan on `m` is closed (so mutating `m` afterwards is legal) and
    // `&p` is the address of a plain local (not HYB-E3105).
    let r = check(
        "struct cell { n int }\n\
         fn touch(ptr *cell) {\n\
             ptr.n = 99\n\
         }\n\
         fn main() {\n\
             m := map[string]cell{\"a\": cell{n: 1}, \"b\": cell{n: 2}}\n\
             p := &m[\"a\"]\n\
             p.n = 10\n\
             p = cell{n: 20}\n\
             m[\"c\"] = cell{n: 30}\n\
             touch(&p)\n\
             q := &m[\"a\"]\n\
             q.n = 11\n\
             ok := false\n\
             q, ok = m[\"b\"]\n\
             _ = ok\n\
             m[\"d\"] = cell{n: 40}\n\
             touch(&q)\n\
         }\n",
    );
    assert!(codes(&r).is_empty(), "{:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}


