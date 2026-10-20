// ============================================================================
// type-name-in-value-position-controls-accept — Checker corpus (issue #778).
//
// PURPOSE: the ACCEPT half of the type-name-in-value-position rule. Every
// shape here uses a type name in a legal TYPE position, or shadows it with a
// real value, and each was verified to build before this fixture was committed.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE); SPEC-LANGUAGE X4
// (conversions are explicit, spelled `T(x)`).
//
// ---- Why this is an ACCEPT ----
//  1. The CALLEE of a call is a call position, not a value position: a
//     built-in conversion (`int(3.0)`, `u8(200)`) puts a type name there
//     legally. A rule that rejected every type-named identifier would break
//     all of these.
//
//     NOTE: `holder(1)` — a struct conversion with a positional argument — is
//     NOT used here. It is accepted by the checker but fails `go build` with
//     `cannot convert 1 (untyped int constant) to type holder`. That is a
//     pre-existing, separate defect (validating a conversion's target against
//     its argument), identical on main before this change, so pinning it as an
//     ACCEPT would have been wrong and pinning it as a REJECT would have
//     widened this fixture's scope. It is reported on the issue instead.
//  2. `new(T)` and `make(T, ...)` take a TYPE as their FIRST argument. This is
//     the control that matters most: an early version of the #778 check walked
//     that argument as a value and broke `n5-zero-map-first-write.hy`, which
//     uses `new(bag)`. The corpus caught it, which is why it is pinned here.
//     `make`'s later arguments (len, cap) ARE values and still walk.
//  3. A local binding shadows the type name, so `holder := 5` makes the name a
//     value from that point on.
//  4. A field access whose field happens to share a type's name is ordinary.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

struct holder {
    n int
}

fn main() {
    // 1. Conversion callees — a type name is the right spelling here.
    i := int(3.0)
    b := u8(200)
    fmt.println("{i} {b}")

    // 2. new/make take a type first.
    p := new(holder)
    p.n = 7
    c := make(chan int, 1)
    c <- 1
    s := make([]int, 4)
    fmt.println("{p.n} {<-c} {len(s)}")

    // 3. A struct literal, and a field whose name matches a type.
    lit := holder{n: 2}
    fmt.println("{lit.n}")

    // 4. A local shadowing the type name is a value.
    holder := 5
    fmt.println("{holder}")
}
