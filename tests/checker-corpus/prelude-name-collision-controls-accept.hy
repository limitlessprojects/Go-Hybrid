// ============================================================================
// prelude-name-collision-controls-accept — Checker corpus (bounty track: "grow the adversarial corpus").
//
// PURPOSE: the ACCEPT half of the #777 prelude-name-collision rule — programs
// that declare the same reserved NAMES but never pull a prelude in, so nothing
// is redeclared and the file still builds.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is an ACCEPT ----
// The emitter injects a prelude only when the emitted body uses the matching
// surface (`needs_option` keys on `option[` / `some(`; `needs_set` keys on
// `set[` / `.insert(` / `.remove(` / `.contains(`). A declaration is not a use:
//
//   1. `fn set(...)` declared AND called — the call spelling is `set(`, which
//      trips none of the set triggers. `func set(` therefore emits alongside no
//      prelude at all.
//   2. `struct option { n int }` declared and used as an ordinary struct —
//      emits `type option struct {` and `option{n: 5}`, neither of which
//      contains `option[` (no type ARGUMENTS are written) or `some(`.
//   3. `fn len(...)` declared and called — `len` is only captured once the set
//      prelude that calls `len(s.m)` is actually injected, which it is not here.
//
// Rejecting any of these would be over-rejection: each was verified to build
// before this fixture was committed, and an early draft of the rule that
// rejected `set` and `option` unconditionally broke exactly these programs.
//
// ---- Expected verdict (verified to build and run before commit) ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Paired fixtures ----
// `prelude-name-collision-reject.hy` (redeclaration) and
// `prelude-predeclared-capture-reject.hy` (predeclared capture) are the
// rejected twins. `prelude-method-name-controls-accept.hy` covers the case
// where the option prelude IS injected but nothing collides.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn set(dst []map[string]int, v map[string]int) {
    dst[0] = v
}

struct option {
    n int
}

fn len(xs []int) int {
    return 42
}

fn main() {
    d := []map[string]int{map[string]int{"k": 0}}
    set(d, map[string]int{"a": 7})
    o := option{n: 5}
    fmt.println("{o.n} {len([]int{1})} {d}")
}
