//! Regression: the checker's foreign METHOD surface must not bless a name the
//! emitter cannot lower and the vendored Go binding does not ship (#2200).
//!
//! `hyb check` used to accept `doc.get("k")`, `doc.remove("k")` and
//! `doc.to_string_pretty()` on a `serde_json.value` handle, because
//! `foreign::method_mode` listed those three names alongside the real ones.
//! None of them is a method on `jsonbinding.Value` — `get`/`remove` are not
//! shipped at all, and `to_string_pretty` is a PACKAGE function
//! (`serde_json.to_string_pretty(doc)`). Every one of them therefore passed
//! `hyb check` and then died in the toolchain:
//!
//! ```text
//! ./main.go:17:20: doc.Get undefined (type jsonbinding.Value has no field or method Get)
//! ./main.go:17:23: doc.Remove undefined (type jsonbinding.Value has no field or method Remove)
//! ./main.go:17:20: doc.ToStringPretty undefined (type jsonbinding.Value has no field or method ToStringPretty)
//! ```
//!
//! That violates GUARANTEES.md row 13 (an accepted program must emit Go that
//! survives `go build`). These tests pin the rejection of the phantom names and
//! the acceptance of the five real ones, so the table cannot drift back.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

/// A `serde_json.value` handle, bound so the emitter/checker both see a handle.
fn program(call: &str) -> String {
    format!(
        "import \"rust/serde_json\"\n\
         fn main() {{\n\
             doc := serde_json.parse(`{{}}`)?\n\
             {call}\n\
         }}\n"
    )
}

#[test]
fn phantom_serde_json_methods_are_rejected() {
    // Each of these passed `hyb check` with 0 diagnostics before the fix and
    // then failed `go build` with `... has no field or method ...`.
    for call in [
        "v := doc.get(\"k\")?\n    fmt.println(\"{v}\")",
        "doc.remove(\"k\")?\n    fmt.println(\"ok\")",
        "s := doc.to_string_pretty()?\n    fmt.println(\"{s}\")",
    ] {
        let r = check(&program(call));
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "phantom method call must reject, got {:?}",
            r.diagnostics
        );
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn real_serde_json_methods_still_accept() {
    // The five methods `jsonbinding.Value` actually ships. `array`/returned
    // views and `set`/mutations are covered by the A2/A7 suites; this test only
    // guards that the surface tightening did not over-reject the surface itself.
    for call in [
        "a := doc.array(\"k\")?\n    fmt.println(\"n={len(a)}\")",
        "n := doc.int(\"k\")?\n    fmt.println(\"n={n}\")",
        "s := doc.string(\"k\")?\n    fmt.println(\"s={s}\")",
        "doc.set(\"k\", 1)?\n    fmt.println(\"ok\")",
        "doc.push_string(\"x\")?\n    fmt.println(\"ok\")",
    ] {
        let r = check(&program(call));
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "real method `{call}` must accept, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn package_form_to_string_pretty_is_not_a_method() {
    // `to_string_pretty` is a package function; the method spelling must be
    // rejected while the package spelling keeps working (example 05's shape).
    let reject = check(&program("s := doc.to_string_pretty()?\n    fmt.println(\"{s}\")"));
    assert_eq!(reject.verdict, Verdict::Reject, "got {:?}", reject.diagnostics);

    let accept = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{{}}`)?\n\
             fmt.println(serde_json.to_string_pretty(doc)?)\n\
         }\n",
    );
    assert_eq!(
        accept.verdict,
        Verdict::Accept,
        "package form must accept, got {:?}",
        accept.diagnostics
    );
}

#[test]
fn arbitrary_unknown_method_on_a_handle_is_rejected() {
    // The general shape behind issue #2200 itself, independent of the three
    // names above: ANY name outside the binding surface must fail closed.
    let r = check(&program(
        "doc.totally_fake_method(\"x\")?\n    fmt.println(\"ok\")",
    ));
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {:?}",
        codes(&r)
    );
    // The diagnostic must name the offending method and list the real surface,
    // so the fix is actionable rather than a bare "no".
    let msg = &r.diagnostics[0].message;
    assert!(msg.contains("totally_fake_method"), "message: {msg}");
    let notes = r.diagnostics[0].notes.join(" | ");
    assert!(
        notes.contains("array, int, string, set, push_string"),
        "notes must list the real surface, got: {notes}"
    );
}

#[test]
fn view_shape_stays_fail_open() {
    // A `serde_json.view` is two different Go types depending on context: a
    // plain `[]jsonbinding.Value` slice (no methods — `len(v)` is the call
    // form) or an element `jsonbinding.Value`. The checker cannot tell them
    // apart from the shape alone, so views must NOT be rejected here or legal
    // element code breaks. Pinned so a future tightening does not over-reach.
    let r = check(&program("a := doc.array(\"k\")?\n    fmt.println(\"n={len(a)}\")"));
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "view `len` must stay accepted, got {:?}",
        r.diagnostics
    );
}

// --- same bug class, different handle: sync.Mutex / sync.RWMutex ---------
//
// `sync.mutex` resolves to `ForeignOpaque("sync.mutex")`, and because the
// method tables were fail-open, `s.mu.totally_fake()` passed `hyb check` and
// then died in the toolchain:
//
//     s.mu.totally_fake undefined (type "sync".Mutex has no field or method totally_fake)
//
// These pin both directions: the phantom name rejects, the real Go surface
// (Lock/Unlock/TryLock, and the RW quartet) still accepts.

#[test]
fn phantom_sync_mutex_method_is_rejected() {
    let r = check(
        "struct stats {\n\
             mu sync.mutex\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.mu.totally_fake()\n\
             fmt.println(\"n={s.n}\")\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    let msg = &r.diagnostics[0].message;
    assert!(msg.contains("totally_fake"), "message: {msg}");
    assert!(msg.contains("sync.mutex"), "message: {msg}");
}

#[test]
fn real_sync_mutex_methods_still_accept() {
    let r = check(
        "struct stats {\n\
             mu sync.mutex\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.mu.lock()\n\
             s.n = 5\n\
             s.mu.unlock()\n\
             fmt.println(\"n={s.n}\")\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}

#[test]
fn phantom_sync_rwmutex_method_is_rejected_but_real_ones_accept() {
    let bad = check(
        "struct stats {\n\
             rw sync.rwmutex\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.rw.totally_fake()\n\
             fmt.println(\"n={s.n}\")\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "got {:?}", bad.diagnostics);

    let good = check(
        "struct stats {\n\
             rw sync.rwmutex\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.rw.rlock()\n\
             v := s.n\n\
             s.rw.runlock()\n\
             fmt.println(\"v={v}\")\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "got {:?}", good.diagnostics);
}

#[test]
fn pointer_and_snake_case_sync_mutex_receivers_enforce_surface() {
    for decl in [
        "mu *sync.mutex",
        "mu sync.rw_mutex",
        "mu *sync.rw_mutex",
        "mu *sync.rwmutex",
    ] {
        let bad = check(&format!(
            "struct stats {{\n    {decl}\n    n int\n}}\n\
             fn main() {{\n    var s stats\n    s.mu.totally_fake()\n    fmt.println(\"n={{s.n}}\")\n}}\n"
        ));
        assert_eq!(
            bad.verdict,
            Verdict::Reject,
            "phantom method on `{decl}` must reject, got {:?}",
            bad.diagnostics
        );
        assert!(
            codes(&bad).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for `{decl}`, got {:?}",
            codes(&bad)
        );
    }

    let good = check(
        "struct stats {\n\
             rw sync.rw_mutex\n\
             n int\n\
         }\n\
         fn main() {\n\
             var s stats\n\
             s.rw.rlock()\n\
             v := s.n\n\
             s.rw.runlock()\n\
             fmt.println(\"v={v}\")\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "got {:?}", good.diagnostics);
}


// --- Atomics stay fail-open (learned in review) --------------------------
//
// An earlier revision of the #2200 fix also gated the scalar atomics on a
// single ATOMIC_SCALAR list. That was wrong in BOTH directions, because
// receiver_type_name collapses every TypeShape::Atomic onto the one key
// `atomic.int` -- the shape carries no element type, so the checker cannot
// tell `atomic.bool` from `atomic.int64`. Their real Go surfaces differ:
//
//   * Bool has no Add/And/Or        -> the union blesses a phantom method.
//   * the integer atomics DO have And/Or (Go 1.23+), so the intersection
//     rejects real methods that `go build` accepts.
//
// Neither list is correct, so atomics are deliberately left fail-open until
// TypeShape::Atomic can carry its element type. These two tests pin that
// decision so a later revision cannot silently reintroduce a half-correct
// surface.

#[test]
fn atomic_integer_and_or_stay_accepted() {
    // sync/atomic.Int64.And/.Or exist since Go 1.23 and `go build` accepts
    // them. A surface that omits them would be a NEW false rejection.
    let src = concat!(
        "fn main() {\n",
        "    var h atomic.int64\n",
        "    h.store(6)\n",
        "    h.and(3)\n",
        "    h.or(2)\n",
        "    fmt.println(\"v={h.load()}\")\n",
        "}\n",
    );
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}

#[test]
fn atomic_bool_stays_fail_open_rather_than_half_gated() {
    // atomic.Bool has no Add, so this is a real (unfixed) bug of the #2200
    // class -- but it cannot be fixed from the shape alone without breaking
    // atomic.Int64.And. Pinned as KNOWN fail-open: if the surface is later
    // split per element type, this test flipping to Reject is the signal that
    // the shape model was fixed. Update it deliberately; do not delete it.
    let src = concat!(
        "fn main() {\n",
        "    var b atomic.bool\n",
        "    b.store(true)\n",
        "    b.add(true)\n",
        "    fmt.println(\"v={b.load()}\")\n",
        "}\n",
    );
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "atomic surface became type-aware -- if bool now rejects, that is an improvement; update this test and the note in foreign.rs",
    );
}