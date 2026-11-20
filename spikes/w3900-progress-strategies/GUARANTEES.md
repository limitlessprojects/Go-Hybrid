# GUARANTEES.md — progress-strategies spike claims table

**Spike**: `spikes/w3900-progress-strategies`
**Anchor**: WO-2 `VERDICT_2026-11-08T05:40Z`, `limitlessprojects/Go-Hybrid#3900`

## Scope

This document lists the claims made by the progress-strategies attractor/rank verifier spike. It does **not** modify the top-level `GUARANTEES.md` of the Hybrid compiler; it is a local spike claims table scoped to `spikes/w3900-progress-strategies/`.

## Claims table

| # | Surface | What the verifier does | The claim | Evidence |
|---|---------|----------------------|-----------|----------|
| 1 | Attractor search (20 regions) | Finite-state game attractor search over each pre-registered region | **Guaranteed**: if ADMIT, an attractor exists within budget and all falsifiers pass | `verifier.go` attractor search; `RESULTS.md` 20-region table |
| 2 | Rank certificate | Strictly decreasing rank per progress step | **Guaranteed**: ADMIT regions carry a well-founded, strictly decreasing rank certificate | `verifier.go` rank validation; `RESULTS.md` Rank-OK column |
| 3 | `parallelism_ratio` | Ratio of concurrent to serial progress steps | **Guaranteed**: ADMIT regions satisfy `parallelism_ratio >= k_min > 1` | `verifier.go` F1 falsifier; `RESULTS.md` F1 column |
| 4 | Pre-terminal channel-close | Channel not closed before attractor terminal state | **Guaranteed**: ADMIT regions have no send-after-close on terminal channels | `verifier.go` channel-close validation; `RESULTS.md` Channel-Close-OK column |
| 5 | F1 counterexample search | Single trace with `parallelism_ratio < k_min` or `k_min <= 1` | **Guaranteed**: F1 is P4-falsifiable; a single serial-dominated trace refutes | `verifier_test.go` TestF1Counterexample |
| 6 | F2 counterexample search | M1 Option 1 preemption-free step vs M2 boundary | **Guaranteed**: M1/M2 preemption boundary violation yields REJECT | `verifier_test.go` TestF2PreemptionBoundary |
| 7 | F3 counterexample search | Runtime witness into explicit losing sink | **Guaranteed**: witness reaching losing sink refutes safety | `verifier_test.go` TestF3LosingSink |
| 8 | Row 16 checked-subset boundary | Send/close protocol ordering | **No claim** — row 16 remains outside Tier-0 scope; this spike documents the boundary only | `PREREG.md` §5; top-level `GUARANTEES.md` row 16 |

## Row 16 — checked-subset boundary (extension note)

Top-level `GUARANTEES.md` row 16 (Send-on-closed-channel panics): "The checker proves value sendability, not send/close protocol ordering — **No claim** — a liveness/protocol property, out of Tier-0's scope."

This spike does **not** extend the checker. It documents where the checked-subset boundary lies:

- **Checked subset (in scope)**: value sendability at spawn/send boundaries; strictly decreasing rank; `parallelism_ratio >= k_min > 1`; no send-after-close on channels that the verifier manages.
- **Outside checked-subset (no claim)**: send/close protocol ordering beyond the verifier's managed channels; runtime liveness beyond attractor termination; any behavior involving code the verifier cannot see.
