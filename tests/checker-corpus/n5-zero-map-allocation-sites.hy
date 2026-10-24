// ============================================================================
// n5-zero-map-allocation-sites — Checker corpus (added WITH the fix for the
// #645 follow-up: zero values produced at ALLOCATION sites).
//
// PURPOSE: pin N5 where the zero value is not declared but allocated —
// `make([]T, n)` / `make([]T, n, c)` elements (and the spare capacity a
// later reslice exposes), `make([]map[K]V, n)` elements, and `[N]T` arrays
// in their `var`, literal, const-length, and partially keyed forms. Before
// this fix every one of these programs passed `hyb check`, built, and died
// at its first write with `panic: assignment to entry in nil map`: #662
// allocated zero maps at `var` declarations, struct literals and `new(T)`
// only, and recorded arrays and `make([]bag, n)` as not covered.
//
// RULE CITATION: SPEC-LANGUAGE N5 — "`var m map[k]v; m[k] = v` works (first
//   write allocates), where Go panics on a nil map." N3's zero-value table
//   gives maps "empty (`len` 0; first write allocates — N5)" and a struct
//   "field-wise zero values"; `make([]T, n)` and `[N]T` produce exactly
//   those zero values, n at a time. D11 (keep N5 vs Go-exact) is open; this
//   fixture pins the spec as written, and a D11 reversal changes this
//   verdict by logged amendment.
//
// NOT pinned here (no allocation site — a READ produces the zero value):
//   a missing map element (`mm[k][k2] = v`, `b, ok := bags[k]`) and a
//   receive from a closed channel. Those keep Go's nil today and are
//   tracked separately.
//
// ---- Tier-0 rule set applied ----
// Layer 1 (Go-side GC): `bags`, `maps`, `spare`, `arr`, `lit`, `shelves`,
//   `part`, `boxed` — fresh locals written by one task; `fill` mutates its
//   slice parameter's elements in the caller's task. No spawn or send site,
//   so no sendability class applies.
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// ---- Expected verdict (registered with the fix, 2026-10-23) ----
// EXPECTED-VERDICT: ACCEPT
//   ACCEPT — zero check-sites; each write is one N5 defines as valid.
// EXPECTED-OUTPUT: 0 1 2 3 4 5 6 x 7 3
//
// ---- Annotations / managed ----
// lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

import "fmt"

struct bag {
    label  string
    counts map[string]int
}

struct shelf {
    bags []bag
}

const n_shelves = 2

fn fill(bs []bag) {
    for i in 0..len(bs) {
        bs[i].counts["k"] = i + 1
    }
}

fn main() {
    // make([]T, n): every element is a zero value, so its map allocates.
    bags := make([]bag, 2)
    bags[1].counts["a"] = 1

    // make([]map[K]V, n): the elements are zero maps.
    maps := make([]map[string]int, 1)
    maps[0]["b"] = 2

    // make([]T, n, c): the spare capacity holds zero values too, and a
    // reslice within the capacity exposes them.
    spare := make([]bag, 0, 4)
    spare = spare[:1]
    spare[0].counts["c"] = 3

    // A zero array in its var, literal, const-length and partial forms.
    var arr [2]bag
    arr[0].counts["d"] = 4
    lit := [2]bag{}
    lit[1].counts["e"] = 5
    var shelves [n_shelves]bag
    shelves[1].counts["f"] = 6
    part := [2]bag{bag{label: "x"}}
    part[1].counts["g"] = 7

    // make inside a struct literal; the elements reach a callee intact.
    boxed := shelf{bags: make([]bag, 2)}
    fill(boxed.bags)

    // One stdout line so the differential loop's single-line
    // EXPECTED-OUTPUT assertion can pin it byte-exact.
    fmt.println(len(bags[0].counts), bags[1].counts["a"], maps[0]["b"], spare[0].counts["c"], arr[0].counts["d"], lit[1].counts["e"], shelves[1].counts["f"], "{part[0].label} {part[1].counts["g"]}", boxed.bags[0].counts["k"] + boxed.bags[1].counts["k"])
}
