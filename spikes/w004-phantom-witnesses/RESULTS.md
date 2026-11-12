# W-004 spike: phantom capability witnesses (zero-cost use-after-move in emitted Go)

Ledger row: W-004 (#174) · capabilities-replace-the-alias-graph family.
Author: maintainers (Super Z), session-18. Pre-registration committed with the
artifact in the same commit — no post-hoc edits to the P1–P4 statements.

## Pre-registration (committed before running)

- **P1**: a witnessed move costs zero allocations and is indistinguishable
  from plain assignment in ns/op. *Falsified if* allocs/op > 0 or a stable
  >2x time delta vs the baseline.
- **P2**: task/channel boundaries can take live witnesses only; the W-005
  clone seam (`hyClone`) re-arms a fresh witness for the far side.
- **P3**: defer arguments evaluated at defer time carry their own live
  witness snapshot (the encoding expresses the ordering discipline).
- **P4 (declared falsifier)**: closure captures DEFEAT the encoding — a
  captured use after move is invisible to witnesses, and the moved value's
  storage is still shared (map headers are references). *Concluded if* a
  captured use after `hyMove` aliases the receiver's view.

## Results (go 1.26.9, linux/amd64, this sandbox)

- P1 **CONFIRMED**: `BenchmarkWitnessedMove` 0.347 ns/op **0 B/op 0 allocs/op**
  vs `BenchmarkPlainMoveBaseline` 0.356 ns/op 0 B/op 0 allocs/op — inside
  noise; the witness is a zero-size struct argument and compiles away.
- P2 **CONFIRMED**: boundary clone re-arms `Live[R]` on the far side; the
  moved witness never crosses.
- P3 **CONFIRMED**: the deferred closure receives `hyUse(w, d)`'s snapshot;
  the subsequent `hyMove` in the same frame does not affect it.
- P4 **CONFIRMED (the falsifier fired as predicted)**: after `hyMove`, the
  captured `x` and the receiver's `x2` share backing storage
  (`x2["hit"] == 1` after `sink()` runs) and the encoding is silent about the
  captured use. Witnesses enforce the NON-captured subset only.
- Negative battery (`run_negatives.sh`): use-after-move and loop-backedge
  double-move both **rejected by the Go type checker at compile time**
  (tagged files fail `go build` as required).
- `go vet` clean, `gofmt` clean, `go test -race -count=2` clean.

## Verdict for the M1 window (slot 4, 2026-11-01)

1. Zero-runtime-cost E3003-class enforcement for the emitted non-captured
   subset is REAL: the checker becomes a witness producer, `hyb emit` grows
   per-binding `type hyR_x struct{}` + `Live`/`Moved` witnesses + `hyUse`
   guards, and the Go compiler takes over enforcement for free.
2. Witnesses do NOT replace the may-share graph: captures (P4) and storage
   sharing survive the move. The two independent spikes (#370's operational
   take-and-detach P4 and this P4) converge on the same residual — that
   convergence is itself the finding: the capability family's honest scope
   is "free enforcement for the non-captured subset + the existing
   capture-freeze machinery for the rest".
3. Codegen estimate: ~3 synthesized lines per moved binding (marker type,
   shadow block, guard call) — comfortably inside the <250-LOC emitter delta
   the ledger row asked for.

## What would change the verdict

- An emitter prototype that hoists witness variables across loop back-edges
  without re-arming (would break P2's discipline) — not attempted here.
- A capture-encoding that works (e.g. capturing the witness WITH the value in
  the closure environment) — noted as follow-up work, not falsified.
