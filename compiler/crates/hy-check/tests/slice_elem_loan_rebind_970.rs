//! slice_elem_loan_rebind_970.rs — issue #970: a slice-element loan
//! (`&xs[0]`) is not invalidated when its slice is re-bound.
//!
//! `s := &xs[0]` is lowered by hy-emit as an O8 slot loan: the element is
//! copied out at the binding and copied BACK into `xs[i]` at the loan's last
//! use. The store-back is emitted against whatever `xs` names at that point.
//! If the program re-binds `xs` between the loan's creation and its last use,
//! the store-back either indexes past the end of the new slice
//!
//!     panic: runtime error: index out of range [0] with length 0
//!
//! or silently writes the loan's copy into the WRONG element:
//!
//!     xs := []svc{{replicas: 1}, {replicas: 2}}
//!     s := &xs[0]
//!     xs = xs[1:]
//!     s.replicas = 5
//!     // emitted store-back is `xs[0] = s` -> clobbers element 1
//!
//! A3 already invalidates a live slot loan when the loaned container is
//! MUTATED inside the window (`HYB-E3102`). Re-binding the base is the
//! missing half of the same invalidation: the loan's back-edge no longer
//! names the container it was taken from. This file pins the reject side,
//! the ACCEPT side (a re-bind outside the loan's window is legal), and the
//! positive controls that keep the rule from over-reaching.

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

/// Assemble a program around `body` and require a rejection carrying
/// `HYB-E3105` (the A3 re-binding half: "loan invalidated by the re-binding
/// of its base").
fn assert_rejects_rebind(body: &str, what: &str) {
    let src = format!(
        "struct svc {{\n    replicas int\n    tag      string\n}}\n\nstruct folder {{\n    name  string\n    trail []svc\n}}\n\nfn main() {{\n{}}}\n",
        body
    );
    let r = check(&src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3105"),
        "{}: expected HYB-E3105 (live slice-element loan invalidated by the \
         re-binding of its base), got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

/// The counterpart: the program must be accepted with zero errors.
fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.is_empty(),
        "{}: expected ACCEPT with no errors, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "{}", what);
}

// ---------------------------------------------------------------------------
// REJECT: a live slice-element loan whose base is re-bound
// ---------------------------------------------------------------------------

/// Issue #970 repro 1, verbatim shape: `xs = xs[:0]` between the loan's
/// creation and its last use. Emitted Go is `xs[0] = s` on a zero-length
/// slice -> `panic: runtime error: index out of range [0] with length 0`.
#[test]
fn issue_970_repro1_reslice_to_empty_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1, tag: \"a\"}}\n    s := &xs[0]\n    xs = xs[:0]\n    s.replicas = 5\n    fmt.println(\"len {len(xs)}\")\n",
        "issue #970 repro 1 (xs = xs[:0])",
    );
}

/// Issue #970 repro 2: `xs = xs[1:]` — no panic, but the store-back lands on
/// the WRONG element (silent corruption; a runtime bounds check cannot see it).
#[test]
fn issue_970_repro2_reslice_offset_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1, tag: \"a\"}, {replicas: 2, tag: \"b\"}}\n    s := &xs[0]\n    xs = xs[1:]\n    s.replicas = 5\n    fmt.println(\"e0 {xs[0].replicas}\")\n",
        "issue #970 repro 2 (xs = xs[1:])",
    );
}

/// Issue #970 repro 3: `append` past capacity returns a new backing array, so
/// the store-back silently reattaches to the re-bound slice.
#[test]
fn issue_970_repro3_append_realloc_rejects() {
    assert_rejects_rebind(
        "    xs := make([]svc, 0, 1)\n    xs = append(xs, svc{replicas: 1, tag: \"a\"})\n    s := &xs[0]\n    xs = append(xs, svc{replicas: 2, tag: \"b\"})\n    s.replicas = 5\n    fmt.println(\"e0 {xs[0].replicas}\")\n",
        "issue #970 repro 3 (xs = append(xs, ...))",
    );
}

/// A plain slice literal re-binding — the same store-back against a shorter
/// slice. (`xs = other` below is the named-value variant.)
#[test]
fn slice_literal_rebind_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1, tag: \"a\"}}\n    s := &xs[0]\n    xs = []svc{}\n    s.replicas = 5\n    fmt.println(len(xs))\n",
        "xs = []svc{}",
    );
}

/// Re-binding to another slice VALUE (not a slice expression of the base).
#[test]
fn slice_to_other_binding_rebind_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1, tag: \"a\"}}\n    other := []svc{}\n    s := &xs[0]\n    xs = other\n    s.replicas = 5\n    fmt.println(len(xs))\n",
        "xs = other",
    );
}

/// The issue's second affected shape: the base is reached through a path
/// (`&f.trail[0]`), and `f.trail` is re-bound.
#[test]
fn path_base_field_rebind_rejects() {
    assert_rejects_rebind(
        "    f := folder{name: \"a\", trail: []svc{{replicas: 1, tag: \"a\"}}}\n    s := &f.trail[0]\n    f.trail = []svc{}\n    s.replicas = 5\n    fmt.println(len(f.trail))\n",
        "f.trail = []svc{}",
    );
}

/// A whole-struct re-binding replaces the field the loan was taken from.
#[test]
fn path_base_whole_struct_rebind_rejects() {
    assert_rejects_rebind(
        "    f := folder{name: \"a\", trail: []svc{{replicas: 1, tag: \"a\"}}}\n    s := &f.trail[0]\n    f = folder{name: \"b\", trail: []svc{}}\n    s.replicas = 5\n    fmt.println(f.name)\n",
        "f = folder{...}",
    );
}

/// The loan is re-bound rather than defined (`s = &xs[0]` on a later line),
/// so the window starts at an Assign, not a Local.
#[test]
fn assign_form_loan_then_rebind_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1, tag: \"a\"}}\n    var s svc\n    s = &xs[0]\n    xs = xs[:0]\n    s.replicas = 5\n    fmt.println(len(xs))\n",
        "s = &xs[0] then xs = xs[:0]",
    );
}

// ---------------------------------------------------------------------------
// ACCEPT: the re-bind is OUTSIDE the loan's non-lexical window
// ---------------------------------------------------------------------------

/// The loan's last use precedes the re-bind, so the store-back is emitted
/// before the new slice value exists. This is the non-lexical twin of
/// repro 1: a lexical (block-scoped) window would wrongly reject it.
#[test]
fn rebind_after_loan_last_use_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    s.replicas = 5\n    xs = xs[:0]\n    fmt.println(len(xs))\n}\n",
        "re-bind after the loan's last use",
    );
}

// ---------------------------------------------------------------------------
// ACCEPT: positive controls — the rule must not over-reach
// ---------------------------------------------------------------------------

/// Control 1 (task-mandated): re-binding a slice with NO live element loan
/// must still accept.
#[test]
fn rebind_with_no_loan_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    xs = xs[:0]\n    xs = append(xs, svc{replicas: 2})\n    fmt.println(len(xs))\n}\n",
        "re-bind with no loan",
    );
}

/// Control 2 (task-mandated): ordinary `&xs[0]` use must still work.
#[test]
fn ordinary_slice_element_loan_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    s.replicas = 9\n    fmt.println(xs[0].replicas)\n}\n",
        "ordinary &xs[0] read-modify-write",
    );
}

/// Control 3: re-binding a DIFFERENT field of the same struct root does not
/// touch the slice the loan was taken from.
#[test]
fn sibling_field_rebind_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nstruct folder {\n    name  string\n    trail []svc\n}\n\nfn main() {\n    f := folder{name: \"a\", trail: []svc{{replicas: 1}}}\n    s := &f.trail[0]\n    f.name = \"b\"\n    s.replicas = 5\n    fmt.println(f.name)\n}\n",
        "f.name = \"b\" beside a loan on f.trail",
    );
}

/// Control 4: a slice-element loan is DEFINED on a slice, but writing an
/// element and re-binding an unrelated binding stays legal.
#[test]
fn unrelated_binding_rebind_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    ys := []svc{}\n    ys = append(ys, svc{replicas: 3})\n    s.replicas = 5\n    fmt.println(len(ys))\n}\n",
        "re-binding an unrelated slice ys",
    );
}

/// Control 5: a fixed array's element cannot be re-bound to a different
/// backing store, and `&a[0]` keeps its lowering — the array boundary the
/// issue calls out as unaffected.
#[test]
fn fixed_array_element_loan_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    var a [2]svc\n    a[0] = svc{replicas: 1}\n    s := &a[0]\n    s.replicas = 7\n    fmt.println(a[0].replicas)\n}\n",
        "&a[0] on a fixed array",
    );
}

/// Control 6: re-binding an ALIAS of the base is not a re-binding of the
/// base — Go slice values are copied headers, so `q := xs; q = q[:0]`
/// leaves the emitted store-back's `xs` (and its length) untouched.
#[test]
fn alias_rebind_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    q := xs\n    q = q[:0]\n    s.replicas = 5\n    fmt.println(len(q))\n}\n",
        "re-binding the alias q, not the base xs",
    );
}
// ---------------------------------------------------------------------------
// Closure-mediated re-binding: the same invalidation, reached through a call.
// A local closure that re-binds a capture performs the re-binding AT THE CALL
// SITE, so the store-back it invalidates is the same one. A3 already covers
// the closure-mediated MUTATING half (HYB-E3102); these pin the re-binding
// half so the rule does not depend on the re-bind being textually in-window.
// ---------------------------------------------------------------------------

/// A closure that re-binds the loaned base, invoked inside the window.
#[test]
fn closure_rebinds_base_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    f := fn() { xs = xs[:0] }\n    f()\n    s.replicas = 5\n    fmt.println(len(xs))",
        "a closure re-binds the loaned base inside the loan window",
    );
}

/// The closure is defined before the loan but CALLED inside the window: the
/// event is the call, not the definition.
#[test]
fn closure_defined_early_called_in_window_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1}}\n    f := fn() { xs = xs[:0] }\n    s := &xs[0]\n    f()\n    s.replicas = 5\n    fmt.println(len(xs))",
        "a closure defined before the loan but called inside its window",
    );
}

/// An inline fn literal called in place, inside the window.
#[test]
fn inline_closure_rebinds_base_rejects() {
    assert_rejects_rebind(
        "    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    g := fn() { xs = xs[:0] }\n    g()\n    s.replicas = 5\n    fmt.println(len(xs))",
        "an inline closure re-binds the loaned base inside the loan window",
    );
}

/// A closure re-binding an UNRELATED capture leaves the loan alone.
#[test]
fn closure_rebinding_unrelated_capture_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    ys := []svc{{replicas: 9}}\n    s := &xs[0]\n    f := fn() { ys = ys[:0] }\n    f()\n    s.replicas = 5\n    fmt.println(len(ys))\n}\n",
        "a closure re-binds another binding, not the loaned base",
    );
}

/// A closure whose re-bind target is its OWN local is not a capture.
#[test]
fn closure_rebinding_own_local_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    f := fn() {\n        q := []svc{{replicas: 3}}\n        q = q[:0]\n        fmt.println(len(q))\n    }\n    f()\n    s.replicas = 5\n    fmt.println(len(xs))\n}\n",
        "the closure re-binds its own local, not the captured base",
    );
}

/// The same, for a field path: a closure re-binding a SIBLING field is legal.
#[test]
fn closure_rebinding_sibling_field_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nstruct folder {\n    name  string\n    trail []svc\n}\n\nfn main() {\n    f := folder{name: \"a\", trail: []svc{{replicas: 1}}}\n    s := &f.trail[0]\n    g := fn() { f.name = \"b\" }\n    g()\n    s.replicas = 5\n    fmt.println(f.name)\n}\n",
        "the closure re-binds the sibling field, not the loaned field path",
    );
}

/// A closure re-binding the base AFTER the loan window closes stays legal.
#[test]
fn closure_rebinding_after_window_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    s.replicas = 5\n    f := fn() { xs = xs[:0] }\n    f()\n    fmt.println(len(xs))\n}\n",
        "the closure re-binds the base after the loan last use",
    );
}
/// `xs = xs[:]` preserves pointer, length AND capacity, so the store-back
/// still names exactly the slot the loan was taken from — a legal no-op.
#[test]
fn identity_full_reslice_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    xs := []svc{{replicas: 1}}\n    s := &xs[0]\n    xs = xs[:]\n    s.replicas = 5\n    fmt.println(len(xs))\n}\n",
        "the identity re-slice keeps the same header",
    );
}
// ---------------------------------------------------------------------------
// Fixed arrays: caught by the same rule, and MEASURED to be a real divergence.
//
// The rule does not test Array-vs-Slice — it is a whole-value path-prefix
// compare, so a fixed array is caught incidentally rather than by design.
// That is correct, not a side effect to be tolerated: the O8 lowering copies
// the element out at the binding and reads that COPY through the loan, so it
// diverges from true pointer semantics as soon as anything zeroes the array.
//
// Measured on 1f0ff97 (emitted Go vs a real-Go `s := &a[0]` oracle):
//
//   var a [2]svc; a[0] = svc{replicas: 1}; s := &a[0]; a = [2]svc{}
//   read through loan -> hyb emitted: 1      real Go: 0
//   a[0]             -> hyb emitted: 1      real Go: 0
//
// A write-only variant (`s.replicas = 5` then read `a[0]`) happens to agree
// (5 and 5) because the store-back lands in the same zeroed storage — which is
// exactly the trap that makes a naive oracle call this legal. The read-through
// case is the discriminating one, so it is pinned here.
// ---------------------------------------------------------------------------

/// Fixed array, READ through the loan after the re-bind: the loan keeps
/// serving the pre-rebind copy (1) where a real Go pointer sees the zeroed
/// array (0). Diverges — reject.
#[test]
fn fixed_array_read_through_loan_after_rebind_rejects() {
    assert_rejects_rebind(
        "    var a [2]svc\n    a[0] = svc{replicas: 1}\n    s := &a[0]\n    a = [2]svc{}\n    fmt.println(\"read through loan={s.replicas}\")\n    fmt.println(\"a0={a[0].replicas}\")",
        "a fixed array re-bound while its element loan is still read",
    );
}

/// The same shape where the loan is only WRITTEN after the re-bind. This one
/// agrees with real Go (both print 5), but the loan is still read-carrying:
/// the hidden copy is live across the re-bind, so the program is only
/// accidentally right. Kept as an explicit reject so the boundary is the
/// re-bind, not the observation order.
#[test]
fn fixed_array_write_only_after_rebind_rejects() {
    assert_rejects_rebind(
        "    var a [2]svc\n    a[0] = svc{replicas: 1}\n    s := &a[0]\n    a = [2]svc{}\n    s.replicas = 5\n    fmt.println(\"a0={a[0].replicas}\")",
        "a fixed array re-bound while its element loan is still live",
    );
}

/// A fixed array whose loan is fully finished before the re-bind is legal.
#[test]
fn fixed_array_rebind_after_loan_last_use_accepts() {
    assert_accepts(
        "struct svc {\n    replicas int\n}\n\nfn main() {\n    var a [2]svc\n    a[0] = svc{replicas: 1}\n    s := &a[0]\n    s.replicas = 5\n    a = [2]svc{}\n    fmt.println(a[0].replicas)\n}\n",
        "the array is re-bound only after the loan last use",
    );
}

