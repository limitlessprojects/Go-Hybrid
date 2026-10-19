// ============================================================================
// n5-zero-map-first-write — Checker corpus (added WITH the #645 fix).
//
// PURPOSE: pin N5 — a Hybrid-declared map's zero value auto-allocates, so
// the first write to it works where Go panics on a nil map. Before #645
// this program passed `hyb check`, built, and died at its first write with
// `panic: assignment to entry in nil map`: the emitter lowered every zero
// map Go-exact.
//
// RULE CITATION: SPEC-LANGUAGE N5 — "`var m map[k]v; m[k] = v` works (first
//   write allocates), where Go panics on a nil map." T8 says the same; N3's
//   zero-value table gives maps "empty (`len` 0; first write allocates —
//   N5)" and a struct "field-wise zero values", which reaches a map field of
//   a zero-valued, partially keyed, nested, or `new`-allocated struct. D11
//   (keep N5 vs Go-exact) is open; this fixture pins the spec as written,
//   and a D11 reversal changes this verdict by logged amendment.
//
// ---- Tier-0 rule set applied ----
// Layer 1 (Go-side GC): `m`, `b`, `keyed`, `s`, `p`, `t` — fresh locals,
//   written by one task; no spawn or send site, so no sendability class
//   applies.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Expected verdict (registered with the fix, 2026-10-18) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — zero check-sites; each write is one N5 defines as valid.
// EXPECTED-OUTPUT: 1 1 x 3 9 6 7
//
// ---- Annotations / managed ----
// lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "fmt"

type tally = map[string]int

struct bag {
    label  string
    counts map[string]int
}

struct shelf {
    top  bag
    tags tally
}

fn main() {
    // The N5 / T8 example itself.
    var m map[string]int
    m["a"] = 1

    // A map field of a zero-valued struct.
    var b bag
    b.counts["k"] = 2

    // A partially keyed literal leaves the map field at its zero value.
    keyed := bag{label: "x"}
    keyed.counts["x"] = 3

    // Field-wise zero values reach nested structs and aliased map types.
    var s shelf
    s.top.counts["t"] = 4
    s.tags["g"] = 5

    // new(T) yields a pointer to T's zero value.
    p := new(bag)
    p.counts["p"] = 6

    // A compound write is a first write too.
    var t tally
    t["z"] += 7

    // One stdout line so the differential loop's single-line
    // EXPECTED-OUTPUT assertion (pr-441) can pin it byte-exact.
    fmt.println(len(m), len(b.counts), "{keyed.label} {keyed.counts["x"]}", s.top.counts["t"] + s.tags["g"], p.counts["p"], t["z"])
}
