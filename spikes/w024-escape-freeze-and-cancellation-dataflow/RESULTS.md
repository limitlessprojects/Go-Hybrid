# Spike Results: O5.3 Escape-Freeze & W-006 Monotone Cancellation-Epoch Dataflow (#3373 & #352)

## Pre-Registered Hypotheses & Verdicts

| ID | Hypothesis / Falsifier | Target GUARANTEES.md Row(s) | Result | Verdict |
|---|---|---|---|---|
| **H1** | `O5.3` Escape-Freeze rejects post-escape outer writes to `may_share` roots read-captured by foreign Go callbacks (`HYB-E3014`), including via alias peers, while accepting pre-escape initialization + post-escape read sharing with zero `managed` blocks | Row 5 & Row 9 | `3/3` cases (`01` ACCEPT, `02` REJECT `HYB-E3014`, `03` REJECT `HYB-E3014`) | **CONFIRMED** |
| **F1** | Falsifier: synchronous higher-order stdlib callbacks (e.g., `sort.Slice`) reading a map/slice that the caller mutates *after* sorting in the same function will trigger `HYB-E3014` unless scoped or allowlisted | Row 9 | Confirmed: un-narrowed `O5.3` treats all foreign closure escapes as potentially retained (matching `W-016` `HYB-E3013`). Narrowing remedy: block-scoped borrow witness or `managed("...", "interop")` | **FALSIFIER FIRED (remedy documented)** |
| **H3** | Modeling `runtime.SetFinalizer` as a foreign-escape check-site under `O5.3` + `HYB-E3013` rejects finalizers that read state later mutated by the caller (`HYB-E3014`) or write shared state (`HYB-E3013`) | Row 13 | Case `05` REJECT `HYB-E3014`, Case `04` REJECT `HYB-E3013` | **CONFIRMED** |
| **H4** | One-pass monotone `CancellationEpoch` dataflow (`W-006` `#352` / `W-007` `#348`) rejects spawned tasks with uncancelled blocking channel ops or un-checkpointed loop backedges (`HYB-E3210`) in 1 forward pass | Row 12 | `3/3` cases (`06` REJECT `HYB-E3210`, `07` ACCEPT, `08` REJECT `HYB-E3210`) | **CONFIRMED** |

## Proposed `GUARANTEES.md` Row Diffs

| Row | Current Claim | Proposed Claim (with `O5.3` + `W-006` C-Epoch) |
|---|---|---|
| **Row 5 & Row 9** | Guaranteed (`HYB-E3013`) for captured-write escapes; **read-only captures are outside** — Go's DRF-SC with `-race` as the backstop | **Guaranteed** (`HYB-E3013` on captured writes, `HYB-E3014` on post-escape outer writes to captured-read `may_share` roots). Pure read-sharing across caller and callback remains zero-annotation |
| **Row 12** | **No claim** about cancellation semantics | **Guaranteed for the checked task subset** (`HYB-E3210`): every blocking channel op and loop backedge inside a checked `task` observes the lexical cancellation epoch, leaving `managed` as the sole documented blind spot |
| **Row 13** | **No claim** (`runtime.SetFinalizer`) | **Guaranteed** via foreign-escape classification: finalizer callbacks obey `HYB-E3013` (no shared writes) and `HYB-E3014` (`O5.3` escape-freeze on captured reads) |

## Reproducibility

```bash
python3 spikes/w024-escape-freeze-and-cancellation-dataflow/verify_spike.py
```
