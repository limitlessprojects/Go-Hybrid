// ============================================================================
// map-key-comparability-reject — Checker corpus (issue #1565).
//
// PURPOSE: `map[K]V` where `K` has NO `==` in Go. The checker walked the key
// and value types (array lengths, nested maps) but never asked whether the
// key is comparable, so these passed `hyb check` and failed `go build` with
// `invalid map key type ...`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE).
//
// ---- Why this is a REJECT ----
// Go requires map keys to be comparable. The key shape is KNOWN (slice, map,
// closure, array-of-incomparable, struct carrying an incomparable field), so
// this is not an `Unknown` fail-open case. The gate reuses the predicate the
// `==` surface already uses (`incomparable_noun`), including its transitive
// walk through arrays and struct fields.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `map-key-comparability-controls-accept.hy` carries the comparable half
// (`any`, `chan`, pointer, array of comparable, struct of comparable) plus
// the alias resolution both ways (`type z = []int` rejects, `type n = int`
// accepts).
// ============================================================================

struct with_slice {
    xs []int
}

struct nested {
    w with_slice
}

type slice_alias = []int

fn main() {
    // 1. a slice key, on a literal
    a := map[[]int]int{}
    fmt.println(len(a))

    // 2. a function key
    b := map[fn()]int{}
    fmt.println(len(b))

    // 3. a map key
    c := map[map[string]int]int{}
    fmt.println(len(c))

    // 4. an array key whose ELEMENT is incomparable
    d := map[[2][]int]int{}
    fmt.println(len(d))

    // 5. a struct key carrying an incomparable field
    e := map[with_slice]int{}
    fmt.println(len(e))

    // 6. the same, transitively through another struct
    f := map[nested]int{}
    fmt.println(len(f))

    // 7. an alias to a slice
    g := map[slice_alias]int{}
    fmt.println(len(g))
}
