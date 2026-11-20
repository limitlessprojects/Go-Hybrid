// Row 10 comparison: Today's sync.Pool (HYB-E3005) vs W-022 LinearPool[T].
//
// 1. TODAY (REJECT HYB-E3005 — NO CLAIM):
// `sync.Pool` is outside the curated surface; passing a pooled buffer across
// a task boundary or calling `pool.Put(buf)` has no ownership contract, so a
// caller that retains `buf` (or an interior slice `s := buf.Payload[:2]`)
// after `Put` races with the next `Get()` consumer.
package main

// 2. W-022 CAPSULE SURFACE (CHECKED TIER-0 + P4a REMEDY CONDITION):
// - `buf := pool.Get()` introduces a fresh owned root (`Own[PacketBuffer]`).
// - `pool.PutMove(move buf)` is an O5.2 consuming site:
//     (a) `buf` and all `may_share` interior aliases (`s := buf.Payload[:2]`)
//         are frozen at `PutMove`; any post-Put read/write of `buf` or `s`
//         is rejected with `HYB-E3003` (use-after-move).
//     (b) `hy-emit` synthesizes the structural field-reset hook (`clear` on
//         maps/slices) before `sync.Pool.Put`.
