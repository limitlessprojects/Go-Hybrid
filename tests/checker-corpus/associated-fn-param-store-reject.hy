// ============================================================================
// associated-fn-param-store-reject — Checker corpus (bounty track: "grow the
// adversarial corpus").
//
// PURPOSE: an ASSOCIATED function that STORES one parameter into another
//   (`b["a"] = m`) and is called as `Type.name(...)`. The caller then moves
//   the container to a task and writes through the stored alias. Because the
//   call resolves as `Expr::Field` rather than `Expr::Ident`, the
//   interprocedural parameter-store table used to be skipped, so the alias was
//   never recorded and the later write looked unrelated (issue #4407).
//
// RULE CITATION: A1b (freeze an alias when the aliasing value is moved) via
//   O5.2: `holder.insert(b, local_m)` stores `local_m` into `b`; `move b` into
//   the task freezes that storage, so `local_m["k"] = 4` is a write through a
//   live alias of a moved value — HYB-E3003.
//
// ---- Tier-0 rule set applied ----
//   Layer 1 (Go-side GC): `b` (map of map headers), `local_m` (map header,
//     stored into `b["a"]` by the callee), the worker's `b` (moved value).
//   Layer 2 (Rust-side owned): none.
//
// Negative control: the identical program with a plain top-level `fn put(...)`
// is rejected by the pre-existing rule; this fixture pins that the associated
// spelling of the same call gets the same verdict.
// ============================================================================
//
// EXPECTED-VERDICT: REJECT A1b

import "fmt"

struct holder { tag int }

fn holder.insert(b map[string]map[string]int, m map[string]int) {
    b["a"] = m
}

fn worker_nested(b map[string]map[string]int, done chan<- bool) {
    b["a"]["k"] = 2
    done <- true
}

fn main() {
    b := map[string]map[string]int{}
    local_m := map[string]int{"k": 0}
    holder.insert(b, local_m)
    done := make(chan bool, 1)
    task worker_nested(move b, done)
    local_m["k"] = 4
    <-done
    v := local_m["k"]
    fmt.println("{v}")
}
