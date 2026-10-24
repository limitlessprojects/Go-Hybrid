// Row 8 comparison: Today's unchecked string permit vs W-022 MutexCapsule[T].
//
// 1. TODAY (ACCEPT-MANAGED, HYB-E8001 — NO SOUNDNESS CLAIM):
// The compiler accepts the block below solely because the string literal
// says "mutex", even though no lock is ever acquired, causing a data race
// under `go run -race`.
package main

fn worker_today(shared map[string]int) {
    shared["hits"] = shared["hits"] + 1
}

fn today_unsound_managed_permit() {
    shared := make(map[string]int)
    managed("claims mutex protection without locking", "mutex") {
        task worker_today(shared)
        shared["hits"] = shared["hits"] + 1
    }
}

// 2. W-022 CAPSULE SURFACE (CHECKED TIER-0):
// `MutexCapsule[T]` consumes `shared` at construction (`move shared`, O5.2)
// and is itself Class-2 sendable across tasks. Inside each task, mutation is
// only reachable through `WithLock(fn(loan ScopedLoan[map[string]int]))`.
//
// - ACCEPT case: reading/writing `loan` inside the lexical `WithLock` body.
// - REJECT case (HYB-E3102 / O2 + O6): spawning a task with `loan` or storing
//   `loan` into an outer binding escapes the lexical lock window and is
//   rejected fail-closed without needing `managed`.
