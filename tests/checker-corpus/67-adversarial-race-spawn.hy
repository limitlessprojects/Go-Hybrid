// ============================================================================
// 67-adversarial-race-spawn — task spawn boundary soundness (issue #5)
//
// PURPOSE: a closure that WRITES a captured mutable binding, spawned as a
// task. The capture makes the binding an alias shared between main and the
// new task, and main still reads it after the spawn — the exact race shape
// GUARANTEES.md's promise forbids. (PR #402, KonstantinPythonLab, authored
// the adversarial intent in non-Hybrid pseudo-syntax: `func main`,
// `var x: int = 42`, `spawn fn(){}()`, `print(x)`; the maintainer
// integration translated it to Hybrid's actual surface grammar —
// `fn main`, `x := 42`, `task fn() { ... }()`, `fmt.println("{x}")` —
// with the behavioral intent unchanged.)
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): the closure's environment captures `x`; spawning
//   the closure runs that environment on another goroutine while main
//   reads `x` — a data race on a mutable binding.
// Layer 2/3: none.
//
// ---- Sendability / check sites ----
//   [S1] `task fn() { x = 100 }()` — the spawn: the callee is a CLOSURE
//        whose body writes the captured binding (A5's spawn-callee proof).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A1c
//   REJECT — A1c/HYB-E3001: "the spawned callee is a closure: closures
//   capture their environment, so spawning one shares every capture with
//   the new task". Legal routes: a declared top-level `fn` fed over a
//   channel, `task worker(move x)`, or the explicit `managed(...)`
//   hatch (HYB-E8001, counted).
//
// ---- Annotations / managed ----
//   lifetime annotations: 0. `managed` blocks: 0.
// ============================================================================

fn main() {
    x := 42
    task fn() {
        x = 100
    }()
    fmt.println("{x}")
}
