// ============================================================================
// a5-escape-via-user-fn-param-reject — Checker corpus (A5 / HYB-E3013).
//
// PURPOSE: the C9 check-site is laundered through a USER wrapper. A closure
// that writes a captured map is passed to a Hybrid fn / method whose fn-typed
// parameter is then handed to foreign Go (`time.after_func`, which runs it on
// another goroutine). At d8b3fe0 every call below was accepted with
// 0 diagnostics and `go run -race` on the emitted Go reported WARNING: DATA
// RACE against main's write — the same program with `time.after_func` called
// directly is REJECT A5 / HYB-E3013:
//
//   1. `later(fn() { ... })`        — direct wrapper
//   2. `later2(fn() { ... })`       — wrapper of a wrapper (+ a local alias)
//   3. `s.defer_run(fn() { ... })`  — pointer-receiver method wrapper
//
// The checker only treated package-qualified / foreign-receiver callees as
// escape sites, and inside the wrapper the escaping argument is a plain
// parameter (no closure body to inspect), so neither side flagged it.
//
// RULE CITATION: SPEC-LANGUAGE §8.5 C9; GUARANTEES.md Row 9 ("Guaranteed
// (HYB-E3013) for captured-write escapes").
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A5
// EXPECTED-DIAGNOSTIC: HYB-E3013
//
// ---- Paired fixture ----
// `a5-escape-via-user-fn-param-controls-accept.hy`: synchronous callers, a
// wrapper whose foreign call is inside `managed("...", "interop")`, and
// read-only closures keep ACCEPTING.
// ============================================================================

import "time"

fn later(f fn()) {
    g := f
    _ = time.after_func(0, g)
}

fn later2(f fn()) {
    later(f)
}

struct sched {
    n int
}

fn (s *sched) defer_run(f fn()) {
    _ = time.after_func(0, f)
}

fn main() {
    m := map[string]int{}
    s := sched{n: 0}
    later(fn() {
        m["timer"] = 1
    })
    later2(fn() {
        m["timer2"] = 1
    })
    s.defer_run(fn() {
        m["method"] = 1
    })
    m["main"] = 2
    time.sleep(time.milliseconds(50))
    fmt.println("{len(m)}")
}
