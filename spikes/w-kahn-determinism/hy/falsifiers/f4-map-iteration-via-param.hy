// f4-map-iteration-via-param.hy — H1 falsifier for #2475 (sequential).
// HYB-E3506 only tracks maps bound by `x := map[...]` and loops that name
// that same variable. Iterating the map through a parameter evades it.
// SPEC-LANGUAGE T8: maps use Go iteration order, which Go randomizes.
// Correct verdict: REJECT (HYB-E3506).

fn show(t map[string]int) {
    for k, v in t {
        fmt.println("{k}={v}")
    }
}

fn main() {
    scores := map[string]int{"a": 1, "b": 2, "c": 3, "d": 4, "e": 5, "f": 6, "g": 7, "h": 8}
    show(scores)
}
