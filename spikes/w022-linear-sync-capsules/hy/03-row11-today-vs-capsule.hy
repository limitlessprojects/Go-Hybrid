// Row 11 comparison: Today's atomic.Pointer[T] (HYB-E3005, PR #52) vs W-022 AtomicCapsule[T].
//
// 1. TODAY (REJECT HYB-E3005 per PR #52 — NO CLAIM YET):
// PR #52 removed `atomic.Pointer[T]` from Class-2 sendable because `p.Store(&m)`
// makes the pointer swap atomic, NOT the pointee `m`: the sender could keep
// mutating `m` after `Store`, or two tasks could both `p.Load()` and mutate `*p`.
package main

// 2. W-022 CAPSULE SURFACE (CHECKED TIER-0):
// `AtomicCapsule[T]` is Class-2 sendable across tasks because pointee access
// is strictly linear:
// - `cap.StoreMove(move next)` and `prev := cap.SwapMove(move next)` consume
//   `next` under O5.2 (any subsequent sender read/write of `next` or its
//   `may_share` peers rejects as `HYB-E3003`).
// - `owned := cap.Take()` compiles to `ptr.Swap(nil)`, transferring exclusive
//   ownership of the pointee to one task at a time without `Load()` sharing.
