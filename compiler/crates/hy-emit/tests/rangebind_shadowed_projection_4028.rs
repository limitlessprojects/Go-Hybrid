//! Issue #4028: an inner range bound that PROJECTS the shadowed name must keep
//! the outer binding alive.
//!
//! `emit_for` hoists `hyHi0 := hi` before the `for <var> := …` head whenever the
//! name is shadowed and `hi` mentions it. `rangebind::stmt_reads` only counted
//! that hoist for the inclusive form or a call-bearing `hi`, so this:
//!
//! ```hybrid
//! struct box { n int }
//! fn bad(xs []box) {
//!     for a in xs {
//!         for a in 0..a.n { _ = a }
//!     }
//! }
//! ```
//!
//! emitted `for range xs { for a := 0; a < a.n; a++ { … } }` — the bound reads
//! the *new* `a` (an int) before it exists, and Go says
//! `a.n undefined (type int has no field or method n)`.
//!
//! What this file pins:
//!   1. the repro keeps the outer binding and hoists the bound, so the emitted
//!      Go builds;
//!   2. a BARE ident bound (`for v in 0..v`) is deliberately left alone — that
//!      form builds (`v < v` sees the new `v`) and blanking it is what
//!      `rangebind_hi_and_option_match_exhaustion.rs` already pins, so widening
//!      the fix to all mentions would regress it;
//!   3. an index projection (`a[0]`) behaves like the field projection.

use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("pkg.hy", src).expect("parses");
    hy_emit::emit_go_package(&ast, "main", "pkg.hy").expect("emits package")
}

#[test]
fn field_projection_bound_keeps_outer_binding() {
    let go = emit(
        "package main\n\n\
         struct box {\n    n int\n}\n\n\
         fn bad(xs []box) {\n    \
             for a in xs {\n        \
                 for a in 0..a.n {\n            \
                     _ = a\n        \
                 }\n    \
             }\n\
         }\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("for _, a := range xs {"),
        "outer `a` must stay bound so the hoisted bound can read it:\n{go}"
    );
    assert!(
        go.contains("hyHi0 := a.n"),
        "the bound must be hoisted:\n{go}"
    );
    assert!(
        go.contains("for a := 0; a < hyHi0; a++ {"),
        "inner head reads the temp, not the shadowed name:\n{go}"
    );
    assert!(
        !go.contains("a < a.n"),
        "the bound must not be evaluated against the new inner `a`:\n{go}"
    );
}

#[test]
fn bare_ident_bound_still_blanked() {
    // The deliberate non-goal: `for v in 0..v` builds as `for v := 0; v < v; v++`
    // because the condition sees the freshly declared `v`. Blanking the outer
    // binding there is the existing pinned behaviour and must not change.
    let go = emit(
        "package main\n\n\
         fn main() {\n    \
             xs := []int{1, 2}\n    \
             for i, v in xs {\n        \
                 _ = i\n        \
                 for v in 0..v {\n            \
                     _ = v\n        \
                 }\n    \
             }\n\
         }\n",
    );
    assert!(
        go.contains("for i, _ := range xs {"),
        "a bare-ident bound keeps the outer name blanked:\n{go}"
    );
    assert!(
        !go.contains(":= v\n"),
        "no hoist of a bare ident:\n{go}"
    );
}

#[test]
fn index_projection_bound_keeps_outer_binding() {
    let go = emit(
        "package main\n\n\
         fn bad(xs [][]int) {\n    \
             for a in xs {\n        \
                 for a in 0..a[0] {\n            \
                     _ = a\n        \
                 }\n    \
             }\n\
         }\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("for _, a := range xs {"),
        "index projection keeps the outer binding:\n{go}"
    );
    assert!(
        go.contains("hyHi0 := a[0]"),
        "the index bound is hoisted:\n{go}"
    );
}

#[test]
fn unrelated_inner_range_untouched() {
    // A different name in the inner loop must not be affected at all.
    let go = emit(
        "package main\n\n\
         struct box {\n    n int\n}\n\n\
         fn ok(xs []box) {\n    \
             for a in xs {\n        \
                 for j in 0..a.n {\n            \
                     _ = j\n        \
                 }\n    \
             }\n\
         }\n\n\
         fn main() {}\n",
    );
    assert!(
        go.contains("for _, a := range xs {") || go.contains("for i, a := range xs {"),
        "the outer binding is untouched:\n{go}"
    );
    assert!(
        go.contains("a.n"),
        "the bound still references the outer `a`:\n{go}"
    );
}
