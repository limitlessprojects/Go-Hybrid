//! any_field_store_edges.rs — regression tests for issue #1997:
//! `any`-field composites drop non-owned/may-share edges
//! (HYB-E3004 / HYB-E3003).
//!
//! Before the fix, storing a shared map into an `any`-typed struct field
//! (`h.m = shared`) recorded no alias edge, because the gate in
//! `record_place_store_from_snapshot` only consulted the ROOT struct's
//! declared shape — and `shape_may_share` returns false when the only
//! reference-carrying field is `any` / `TypeShape::Unknown`. So `task
//! worker(move h)` froze only `h`, and the later `shared["main"]++` was
//! accepted, contradicting GUARANTEES.md O5.2. The same dropped edge
//! showed up through a composite literal (`box{f: p}`), a shallow
//! struct copy (`s2 := s`), a parallel assignment, and an index store
//! (`xs[0]`). The closure half is the same class of dropped edge for
//! A3: a slot loan captured only through an alias (`p := s`) or only
//! through a copied closure (`f2 := f`) had its window end before the
//! closure's last use, so an insert inside the window was accepted.
//!
//! The fix gates the store on the STORED VALUE's may-share status as
//! well as the root's, and extends slot-loan windows through alias
//! captures and closure copies. The scalar-`any` control and the
//! rebind-then-copy case below pin the non-over-strict side: storing a
//! scalar into an `any` field still accepts, and rebinding the source
//! after the embed severs the edge (the later write targets the NEW
//! map, the moved value holds the OLD one).

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

fn assert_code(r: &hy_check::CheckResult, code: &str, what: &str) {
    let cs = codes(r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {} ({}), got {:?}",
        code,
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(r: &hy_check::CheckResult, what: &str) {
    assert_eq!(
        codes(r),
        Vec::<String>::new(),
        "expected accept ({}), got {:?}",
        what,
        codes(r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// Rejects — the shared edge is materialised through every store form
// ---------------------------------------------------------------------------

#[test]
fn field_store_spawn_is_e3003() {
    // #1997 form 1: field store, then task spawn with move.
    let r = check(
        r#"struct holder { m any }
fn worker(h holder) { h.m = h.m }
fn main() {
    shared := map[string]int{}
    var h holder
    h.m = shared
    task worker(move h)
    shared["main"]++
}
"#,
    );
    assert_code(&r, "HYB-E3003", "field store + spawn");
}

#[test]
fn field_store_channel_send_is_e3003() {
    // #1997 form 2: field store, then channel send with move (the
    // field-store route from the issue's comment, channel-send variant).
    let r = check(
        r#"struct holder { item any }
fn consumer(ch <-chan holder, done chan<- bool) {
    h := <-ch
    fmt.println("item {h.item}")
    done <- true
}
fn main() {
    shared := map[string]int{"a": 1}
    var h holder
    h.item = shared
    ch := make(chan holder, 1)
    done := make(chan bool, 1)
    task consumer(ch, done)
    ch <- move h
    shared["main"] = 2
    <-done
    fmt.println("done")
}
"#,
    );
    assert_code(&r, "HYB-E3003", "field store + channel send");
}

#[test]
fn index_store_into_any_field_is_e3003() {
    // #1997 form 3: the stored value itself comes out of an `any`
    // collection (`xs[0]`), so the edge must flow through the index.
    let r = check(
        r#"struct holder { item any }
fn worker(h holder) { h.item = h.item }
fn main() {
    shared := map[string]int{"a": 1}
    xs := []any{shared}
    var h holder
    h.item = xs[0]
    task worker(move h)
    shared["main"] = 1
}
"#,
    );
    assert_code(&r, "HYB-E3003", "index store into any field");
}

#[test]
fn literal_embed_is_e3004() {
    // #1997 form 4: composite literal embedding a may-share parameter.
    let r = check(
        r#"struct box { f any }
fn worker(s box) { s.f = s.f }
fn leak(p any) {
    s := box{f: p}
    task worker(move s)
}
fn main() {
    m := map[string]int{"k": 1}
    leak(m)
    m["k"] = 2
}
"#,
    );
    assert_code(&r, "HYB-E3004", "literal embed");
}

#[test]
fn struct_copy_is_e3003() {
    // #1997 form 5: shallow struct copy carries the edge to the copy.
    let r = check(
        r#"struct box { f any }
fn worker(s box) { s.f = s.f }
fn main() {
    m := map[string]int{"k": 1}
    s := box{f: m}
    s2 := s
    task worker(move s2)
    m["k"] = 2
}
"#,
    );
    assert_code(&r, "HYB-E3003", "struct copy");
}

#[test]
fn parallel_assign_field_store_is_e3003() {
    // #1997 form 6: the same field store through a parallel assignment.
    let r = check(
        r#"struct holder { m any }
fn worker(h holder) { h.m = h.m }
fn main() {
    shared := map[string]int{}
    var h holder
    var y int
    h.m, y = shared, 2
    task worker(move h)
    shared["main"]++
    fmt.println("{y}")
}
"#,
    );
    assert_code(&r, "HYB-E3003", "parallel-assign field store");
}

#[test]
fn typed_field_control_still_e3003() {
    // Control: the concretely-typed twin already rejected before the
    // fix and must keep rejecting (the fix must not change its code).
    let r = check(
        r#"struct holder { m map[string]int }
fn worker(h holder) { h.m = h.m }
fn main() {
    shared := map[string]int{}
    var h holder
    h.m = shared
    task worker(move h)
    shared["main"]++
}
"#,
    );
    assert_code(&r, "HYB-E3003", "typed field control");
}

// ---------------------------------------------------------------------------
// Rejects — the closure half: A3 windows through alias / copy
// ---------------------------------------------------------------------------

#[test]
fn closure_alias_capture_only_is_e3102() {
    // #1997 closure form 1: the slot loan is captured only through an
    // alias (`p := s`); the insert sits between the closure's creation
    // and its only call. The window must extend to the call.
    let r = check(
        r#"struct acct { count int }
fn main() {
    m := map[string]acct{"a": acct{count: 1}}
    s := &m["a"]
    p := s
    f := fn() { p.count = 99 }
    m["b"] = acct{count: 2}
    f()
}
"#,
    );
    assert_code(&r, "HYB-E3102", "closure alias-only capture");
}

#[test]
fn closure_copy_only_is_e3102() {
    // #1997 closure form 2: the closure itself is copied (`f2 := f`)
    // and only the copy is called; the window must extend to the
    // copy's last use.
    let r = check(
        r#"struct acct { count int }
fn main() {
    m := map[string]acct{"a": acct{count: 1}}
    s := &m["a"]
    f := fn() { s.count = 99 }
    f2 := f
    m["b"] = acct{count: 2}
    f2()
}
"#,
    );
    assert_code(&r, "HYB-E3102", "closure copy-only call");
}

// ---------------------------------------------------------------------------
// Accepts — the fix must not over-reach
// ---------------------------------------------------------------------------

#[test]
fn scalar_any_field_store_accepts() {
    // #1997 control: a scalar stored into an `any` field carries no
    // shared storage, so the move stays exclusive and the program
    // accepts.
    let r = check(
        r#"struct holder { m any }
fn worker(h holder) { h.m = h.m }
fn main() {
    var h holder
    h.m = 1
    task worker(move h)
    fmt.println("ok")
}
"#,
    );
    assert_accept(&r, "scalar any field store");
}

#[test]
fn closure_insert_after_call_accepts() {
    // Positive twin of the closure cases: the insert comes AFTER the
    // closure's last use, so the window has ended and the program
    // accepts.
    let r = check(
        r#"struct acct { count int }
fn main() {
    m := map[string]acct{"a": acct{count: 1}}
    s := &m["a"]
    p := s
    f := fn() { p.count = 99 }
    f()
    m["b"] = acct{count: 2}
}
"#,
    );
    assert_accept(&r, "closure insert after call");
}

#[test]
fn rebind_after_embed_severs_edge_accepts() {
    // Rebinding the source after the embed severs the edge: the later
    // write targets the NEW map while the moved value holds the OLD
    // one, so no shared storage is written and the program accepts.
    let r = check(
        r#"struct box { f any }
fn worker(s box) { s.f = s.f }
fn main() {
    m := map[string]int{"k": 1}
    s := box{f: m}
    m = map[string]int{"k": 9}
    s2 := s
    task worker(move s2)
    m["k"] = 2
}
"#,
    );
    assert_accept(&r, "rebind after embed");
}
