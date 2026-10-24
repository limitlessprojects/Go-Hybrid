// ============================================================================
// recursive-value-type-controls-accept — Checker corpus (issue #1566).
//
// PURPOSE: the indirections that make a self-referential type legal Go must
// keep passing. The reject half is `recursive-value-type-reject.hy`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accept side.
//
// ---- Why this is an ACCEPT ----
// A Go struct may refer to its own type through a slice, map or channel
// (and, at the foreign perimeter, a pointer): the field has a fixed size
// because the referent lives elsewhere. `struct slicy { kids []slicy }` is
// the idiomatic tree. Nested non-recursive structs are the plain control.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

struct slicy {
    kids []slicy
}

struct mappy {
    kids map[string]mappy
}

struct channy {
    ch chan channy
}

struct p {
    a int
    s string
}

struct q {
    p p
    n [2]int
}

// Alias controls: an alias is legal as long as its expansion terminates.
type row = []int

type tally = map[string]int

type counts = tally

fn main() {
    var a slicy
    fmt.println(len(a.kids))

    var b mappy
    fmt.println(len(b.kids))

    var c channy
    fmt.println(c.ch)

    var v q
    fmt.println(v.p.a)

    var r row
    r = append(r, 1)
    fmt.println(len(r))

    m := counts{}
    m["a"] = 1
    fmt.println(len(m))
}
