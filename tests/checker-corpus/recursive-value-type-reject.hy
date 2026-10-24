// ============================================================================
// recursive-value-type-reject — Checker corpus (issue #1566).
//
// PURPOSE: a struct that contains itself BY VALUE. Go rejects the type
// declaration itself (`invalid recursive type: node refers to itself`), and
// nothing in the checker built the containment graph, so the declaration was
// accepted and the emitted module failed `go build`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE).
//
// ---- Why this is a REJECT ----
// The containment graph closes: a direct field, an array element and an
// `option[T]` payload all carry the struct by value. The shape is KNOWN (a
// local struct declaration), so this is not an `Unknown` fail-open case.
// Slice / map / chan / pointer indirections break the cycle and stay legal —
// see `recursive-value-type-controls-accept.hy`.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
// ============================================================================

struct node {
    next node
}

struct tree {
    kids [2]tree
}

struct a {
    b b
}

struct b {
    a a
}

type n = node

struct aliased {
    x n
}

// The alias leg: an ALIAS that expands to itself is invalid through ANY
// constructor — a slice does not break it the way it breaks a defined type.
type self_slice = []self_slice

type loop_a = loop_b

type loop_b = loop_a

fn main() {
    var v node
    fmt.println(v.next.next)
}
