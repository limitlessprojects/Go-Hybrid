// ============================================================================
// map-key-comparability-controls-accept — Checker corpus (issue #1565).
//
// PURPOSE: every legal Go map key must keep working after the #1565 gate.
// Each shape here builds with `go build` today; the reject half is
// `map-key-comparability-reject.hy`.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — accept side.
//
// ---- Why this is an ACCEPT ----
// Go's comparable set includes: booleans, numbers, strings, pointers,
// channels, interfaces (compared dynamically), and arrays/structs whose
// elements/fields are comparable. Type parameters are unresolved in the
// checker and stay fail-open. `any` is the load-bearing control: it is an
// interface type (nilable, comparable), not an unresolved shape.
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

struct comparable_key {
    a int
    s string
}

type int_alias = int

fn main() {
    var by_int map[string]int
    fmt.println(len(by_int))

    var by_any map[any]int
    fmt.println(len(by_any))

    var by_chan map[chan int]int
    fmt.println(len(by_chan))

    var by_ptr map[*comparable_key]int
    fmt.println(len(by_ptr))

    var by_array map[[2]int]int
    fmt.println(len(by_array))

    var by_struct map[comparable_key]int
    fmt.println(len(by_struct))

    var by_alias map[int_alias]int
    fmt.println(len(by_alias))
}
