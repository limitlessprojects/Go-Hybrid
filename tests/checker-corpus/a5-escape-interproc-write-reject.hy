// ============================================================================
// a5-escape-interproc-write-reject — Checker corpus (A5 / HYB-E3013).
//
// PURPOSE: a closure handed to foreign Go (`time.after_func` runs it on
// another goroutine) writes a captured mutable binding ONE CALL AWAY instead
// of with an inline store. Each shape below was accepted with 0 diagnostics
// at d8b3fe0 and `go run -race` on the emitted Go reported WARNING: DATA
// RACE against the main goroutine's write:
//
//   1. a top-level fn that stores through its map parameter (`bump(m, k)`)
//   2. a pointer-receiver user method on a captured struct whose map field
//      it writes (`c.add(k)`)
//   3. a pointer-receiver user method writing a plain scalar field
//      (`s.incr()` — Go auto-addresses the captured `s`)
//   4. a call to another local closure that writes the capture (`inner()`)
//   5. a closure FACTORY call passed inline (`make_cb(m)`) — the named form
//      `cb := make_cb(m); time.after_func(0, cb)` was already rejected
//
// The inline-store spelling (`fn() { m["k"] = 1 }`) and the named-closure
// spellings of 1 and 4 were already rejected; `arg_closure_captured_write` /
// `call_mutates_captured` only modelled builtins, foreign Mutate calls and a
// fixed method-name list, so the user-fn / user-method / local-closure /
// inline-factory forms escaped C9.
//
// RULE CITATION: SPEC-LANGUAGE §8.5 C9; GUARANTEES.md Row 9 ("Guaranteed
// (HYB-E3013) for captured-write escapes").
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A5
// EXPECTED-DIAGNOSTIC: HYB-E3013
//
// ---- Paired fixture ----
// `a5-escape-interproc-controls-accept.hy` holds the read-only / by-value /
// closure-local controls that must keep ACCEPTING.
// ============================================================================

import "time"

fn bump(m map[string]int, k string) {
    m[k] = m[k] + 1
}

struct counter {
    hits map[string]int
}

fn (c *counter) add(k string) {
    c.hits[k] = c.hits[k] + 1
}

struct stats {
    n int
}

fn (s *stats) incr() {
    s.n = s.n + 1
}

fn make_cb(m map[string]int) fn() {
    return fn() {
        m["factory"] = 1
    }
}

fn main() {
    m := map[string]int{}
    c := counter{hits: map[string]int{}}
    s := stats{n: 0}
    inner := fn() {
        m["inner"] = 1
    }
    _ = time.after_func(0, fn() {
        bump(m, "timer")
    })
    _ = time.after_func(0, fn() {
        c.add("timer")
    })
    _ = time.after_func(0, fn() {
        s.incr()
    })
    _ = time.after_func(0, fn() {
        inner()
    })
    _ = time.after_func(0, make_cb(m))
    bump(m, "main")
    c.add("main")
    s.incr()
    time.sleep(time.milliseconds(50))
    fmt.println("{len(m)} {len(c.hits)} {s.n}")
}
