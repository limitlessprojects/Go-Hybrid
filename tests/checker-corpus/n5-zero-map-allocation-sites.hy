// n5-zero-map-allocation-sites — Checker corpus (added WITH the fix for the #645 follow-up, #1206 part 1).
// PURPOSE: pin N5 where the zero value is ALLOCATED rather than declared — `make([]T, n[, c])` elements (and
// the spare capacity a reslice exposes), `make([]map[K]V, n)`, `[N]T` arrays (var / literal / const length /
// partial literal), and `make` inside a struct literal reaching a callee. Before the fix each site passed
// `hyb check`, built, and panicked `assignment to entry in nil map`. RULE CITATION: SPEC-LANGUAGE N5 + N3's
// zero-value table (D11 open; pins the spec as written). READ-produced zero values are deliberately not pinned.
// EXPECTED-VERDICT: ACCEPT
// EXPECTED-OUTPUT: 0 1 2 3 4 5 6 x 7 3
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
    bags := make([]bag, 2)                  // make([]T, n): every element is a zero value
    bags[1].counts["a"] = 1
    maps := make([]map[string]int, 1)       // make([]map[K]V, n): the elements are zero maps
    maps[0]["b"] = 2
    spare := make([]bag, 0, 4)              // make([]T, n, c): the spare capacity holds zero values too
    spare = spare[:1]
    spare[0].counts["c"] = 3
    var arr [2]bag                          // zero array: var form
    arr[0].counts["d"] = 4
    lit := [2]bag{}                         // literal form
    lit[1].counts["e"] = 5
    var shelves [n_shelves]bag              // const length
    shelves[1].counts["f"] = 6
    part := [2]bag{bag{label: "x"}}         // partial literal: trailing elements are zero
    part[1].counts["g"] = 7
    boxed := shelf{bags: make([]bag, 2)}    // make inside a struct literal; the elements reach a callee intact
    fill(boxed.bags)
    fmt.println(len(bags[0].counts), bags[1].counts["a"], maps[0]["b"], spare[0].counts["c"], arr[0].counts["d"], lit[1].counts["e"], shelves[1].counts["f"], "{part[0].label} {part[1].counts["g"]}", boxed.bags[0].counts["k"] + boxed.bags[1].counts["k"])
}
