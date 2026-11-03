// ============================================================================
// a5-escape-interproc-controls-accept — Checker corpus (A5 / HYB-E3013).
//
// PURPOSE: the controls for `a5-escape-interproc-write-reject.hy`. A closure
// passed to foreign Go that calls user fns / methods / local closures which
// do NOT write captured storage keeps ACCEPTING:
//   - a top-level fn that only reads its map parameter
//   - a fn that reassigns its by-value int parameter (a copy)
//   - a pointer-receiver method that only reads, and a value-receiver method
//   - a mutating fn applied to a CLOSURE-LOCAL map (no capture written)
//   - a local closure with no captured writes
//   - an inline factory whose returned closure only reads its parameter
//
// RULE CITATION: SPEC-LANGUAGE §8.5 C9 (only captured WRITES need
// `managed("...", "interop")`); GUARANTEES.md Row 9.
//
// EXPECTED-VERDICT: ACCEPT
// ============================================================================

import "time"

fn total(m map[string]int) int {
    return len(m)
}

fn bump(m map[string]int, k string) {
    m[k] = m[k] + 1
}

fn twice(n int) int {
    n = n * 2
    return n
}

struct stats {
    n int
}

fn (s *stats) get() int {
    return s.n
}

fn (s stats) peek() int {
    return s.n
}

fn make_reader(m map[string]int) fn() {
    return fn() {
        _ = len(m)
    }
}

fn main() {
    m := map[string]int{"a": 1}
    s := stats{n: 1}
    pure := fn() {
        _ = 1
    }
    _ = time.after_func(0, fn() {
        _ = total(m)
        _ = twice(3)
        _ = s.get()
        _ = s.peek()
        local := map[string]int{}
        bump(local, "x")
        pure()
    })
    _ = time.after_func(0, make_reader(m))
    time.sleep(time.milliseconds(20))
    fmt.println("{total(m)} {s.get()}")
}
