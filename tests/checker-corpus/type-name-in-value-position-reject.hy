// ============================================================================
// type-name-in-value-position-reject — Checker corpus (issue #778).
//
// PURPOSE: a TYPE NAME used where a VALUE is required.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-accepted
// file must build cleanly with `go build`.
//
// ---- Why this is a REJECT ----
// Go has no value named `holder`, so the emitter printing the identifier
// verbatim gives `holder (type) is not an expression`. The shape is KNOWN (the
// identifier resolves to a struct declaration), so this is not an `Unknown`
// fail-open case: the value does not exist and Go refuses it before the
// program can run.
//
// Four value positions, plus a built-in type and a type alias:
//   * `x := holder`              assignment
//   * `take(holder)`             call argument
//   * `holder + 1`               arithmetic operand
//   * `return holder`            return value
//   * `x := int`                 a built-in conversion target as a value
//   * `x := tally`               a type alias as a value
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT BUILDABLE
// EXPECTED-DIAGNOSTIC: HYB-E1002
//
// ---- Paired fixture ----
// `type-name-in-value-position-controls-accept.hy` carries the accept half.
// The load-bearing controls are the three TYPE positions a type name is legal
// in — a conversion callee (`holder(1)`, `int(3.7)`), the first argument of
// `new`/`make` (`new(bag)`), and anywhere a local binding shadows the type.
// `new(bag)` is not hypothetical: it broke `n5-zero-map-first-write.hy`, which
// is exactly what the corpus exists to catch.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

type tally = map[string]int

struct holder {
    n int
}

fn take(h holder) int {
    return h.n
}

fn make_it() holder {
    return holder
}

fn main() {
    x := holder
    v := take(holder)
    y := holder + 1
    z := int
    t := tally
    fmt.println("{v}")
    fmt.println("{y}")
    fmt.println("{z}")
    fmt.println("{len(t)}")
}
