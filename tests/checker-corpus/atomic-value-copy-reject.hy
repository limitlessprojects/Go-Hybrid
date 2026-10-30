// ============================================================================
// atomic-value-copy-reject — scalar sync/atomic values are copy-locked
// (issue #1399, HYB-E2011 / HYB-E3001 / HYB-E3002).
//
// PURPOSE: pin the checker reject for every by-value copy of a scalar
// sync/atomic binding. `atomic.bool` / `atomic.int32` / `atomic.int64` /
// `atomic.uint32` / `atomic.uint64` / `atomic.uintptr` embed
// `sync/atomic.noCopy` on the Go side: `go vet` (copylocks) rejects the
// copies, and a copy taken while another task mutates through `&hits` is
// an unsynchronized data race under `go run -race`. The checker used to
// mark `TypeShape::Atomic` sendable-by-type (model.rs) and const-copyable
// (sendability.rs), so decl copies (`other := hits`), whole-value
// assignments (`hits = z`), compound read-modify-writes (`hits += 1`),
// by-value task/spawn/send operands, and `move` all passed `hyb check
// --deny-managed` with 0 diagnostics (GUARANTEES.md Row 1 & Invariant 4).
//
// This fixture exercises the STATEMENT-LEVEL leg (HYB-E2011, first
// diagnostic A1c): `other := hits` and `hits = other`. The boundary legs
// (task by value -> HYB-E3001, `ch <- hits` -> HYB-E3002, `move` of a
// holder struct -> HYB-E3001) are pinned by
// `hy-check/tests/atomic_value_copy.rs` (9 tests), and the legal routes
// (`&hits`, `hits.add(1)`, `hits.load()`, pointer copies) are pinned as
// ACCEPT controls there and by corpus #17.
//
// Pre-registered 2026-10-24 against main @ 7d63d5f (issue repro on that
// tree: `hyb check --deny-managed` OK with 0 diagnostics).
//
// EXPECTED-VERDICT: REJECT A1c
// ============================================================================

import "sync/atomic"

fn main() {
    var hits atomic.int64
    hits.add(1)                 // legal: atomic-mediated mutation (OQ6)
    other := hits               // reject: HYB-E2011 — copy-locked by-value copy
    hits = other                // reject: HYB-E2011 — whole-value overwrite
    _ = other
}
