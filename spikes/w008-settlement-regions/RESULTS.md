# W-008 spike: checked settlement for own-handle task regions (cancellation is not completion)

Ledger row: W-008 (#257) · protocol liveness & cancellation family.
Proposal by: luvskyy (Ski).
Spike implementation: hardik4love-lang.
Preregistration committed with the artifact in the same commit.

## Pre-registration (committed before running)

- **P1 (Settlement Completeness)**: In a region with $N$ child tasks and $N$ registered `own` handles,
  `region.WaitSettled()` blocks until all children terminate AND every handle has reached a verified
  terminal state (either `Transferred` into the result bundle or `Released`). Zero dangling handles or unjoined goroutines.
- **P2 (Cancellation Race Invariant - Atomic Handoff)**: When `region.Cancel()` races with child handle acquisition,
  exactly one outcome wins per handle via an atomic handoff transition (`Acquired` or `CancelledBeforeAcquisition`).
  If cancellation wins, the child cannot acquire the handle and the region-level ledger accounts for and releases it.
  If acquisition wins, the child owns the handle and only the child can settle it (cancellation does not preempt live ownership).
- **P3 (Affine Terminal Accounting)**: Any attempt to return `Settled` while a handle remains in `Escrowed` or `Acquired`
  (not released or transferred) fails closed (`ErrIncompleteSettlement`). Receipts are single-use, generation-pinned proofs.
- **P4 (Declared Falsifier / Mutation Battery)**:
  - *Mutation 1 (Omitted cleanup)*: Child acquires handle and exits without transfer/release.
    *Falsified if* `WaitSettled()` returns `Settled` or ignores the missing outcome. (Must fail with `ErrIncompleteSettlement`).
  - *Mutation 2 (Duplicate receipt replay)*: Replaying a stale or duplicate receipt.
    *Falsified if* duplicate receipts are accepted. (Must fail with `ErrDuplicateReceipt`).
  - *Mutation 3 (Cancellation preemption)*: Parent cancellation attempts to forcibly reclaim a child-acquired handle.
    *Falsified if* cancellation mutates an `Acquired` handle into `Released` without child cooperative participation.
  - *Mutation 4 (Cancellation is not completion)*: A child task that blocks permanently must prevent `Settled` certification,
    even after cancellation is requested.

## Results & Verification

- **P1 CONFIRMED (`TestPositiveTrace_NormalCompletion`)**: Both child tasks acquire their handles, do work, and transfer
  ownership to the result bundle. `WaitSettled()` returns `Settled=true`, `TransferredCount=2`, `ReleasedCount=0`.
- **P2 CONFIRMED (`TestPositiveTrace_CancellationBeforeAcquisition`, `TestConcurrency_HandoffRaceIntegrity`)**:
  Parent cancellation before acquisition transitions the atomic handoff to `Cancelled`; child acquisition fails (`TryAcquire() -> false`),
  and the parent ledger synchronously releases the escrowed handles (`ReleasedCount=2`).
  Under 100 concurrent trials of simultaneous acquisition vs cancellation, 100% of handles reached terminal release with zero leaks.
- **P3 CONFIRMED (`TestPositiveTrace_CooperativeCancellationAfterAcquisition`)**:
  When cancellation arrives after child acquisition, the child observes `ctx.Done()` and synchronously invokes `ReleaseHandle()`.
  Region certifies `Settled=true`, `ReleasedCount=1`.
- **P4 CONFIRMED (All declared falsifiers and mutations held)**:
  - *Mutation 1*: Omitted cleanup cleanly rejected with `ErrIncompleteSettlement`.
  - *Mutation 2*: Duplicate transfer cleanly rejected with `ErrDuplicateReceipt`.
  - *Mutation 3*: Parent cancellation after acquisition does not alter child handle state (`StateAcquired` preserved; child completes release).
  - *Mutation 4*: `WaitSettled()` waits on `sync.WaitGroup`, guaranteeing unjoined children prevent early or unsound region completion.

## Verdict for the M1 window (slot 8, 2026-11-01)

1. **The Contract is Sound & Real**: Checked settlement can be enforced strictly as an **ownership-accounting contract**
   without requiring runtime VM support or whole-program model checking.
2. **Cancellation is NOT Completion**: By decoupling cancellation requests from terminal completion, tasks are allowed to
   perform synchronous cleanup and drain their owned resources before the parent scope exits.
3. **Emitter Lowering Footprint**:
   Lowering an opt-in `region` block (`region (h1, h2) { task childA(move h1); task childB(move h2); }`) maps directly to:
   - `region := NewRegion(ctx)` + `defer region.Cancel()`
   - Synthesized atomic handoff slots for each registered handle
   - `defer region.WaitSettled()` ensuring scope cannot exit with dangling resources
   Estimated codegen delta: ~40 lines per region in `hy-emit`.

## What would change the verdict

- Nested dynamic spawning inside the region: this spike assumes a fixed finite child set per region (the core restriction in #257).
  Widening to dynamically spawned unbounded trees would require an affine capability pool.
