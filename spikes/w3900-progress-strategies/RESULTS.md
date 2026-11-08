# RESULTS.md — 20-region empirical results

**Spike**: `spikes/w3900-progress-strategies`
**Issue**: `limitlessprojects/Go-Hybrid#3900`
**Generated**: 2026-11-08T12:16:54Z
**Verdict anchor**: `VERDICT_2026-11-08T05:40Z` (WO-2)
**k_min**: 1.5

## 20-region empirical results table

| Region | Seed | Verdict | States | Edges | CPU-s | Ratio | Rank-OK | ChClose-OK | F1 | F2 | F3 |
|--------|------|---------|--------|-------|-------|-------|---------|------------|----|----|----|
| N01 | 0x81 | REJECT | 5 | 9 | 0.000 | 0.500 | true | false | false | false | false |
| N02 | 0x82 | REJECT | 7 | 9 | 0.000 | 0.667 | true | false | false | false | false |
| N03 | 0x83 | REJECT | 6 | 7 | 0.000 | 1.000 | true | true | false | false | false |
| N04 | 0x84 | REJECT | 10 | 16 | 0.000 | 1.000 | true | false | false | false | false |
| N05 | 0x85 | REJECT | 6 | 12 | 0.000 | 1.000 | true | false | false | false | false |
| N06 | 0x86 | REJECT | 5 | 6 | 0.000 | 0.500 | true | false | false | false | false |
| N07 | 0x87 | REJECT | 7 | 11 | 0.000 | 0.667 | true | false | false | false | false |
| N08 | 0x88 | REJECT | 11 | 26 | 0.000 | 8.000 | true | true | true | false | false |
| N09 | 0x89 | REJECT | 6 | 9 | 0.000 | 2.000 | true | true | true | false | false |
| N10 | 0x8a | REJECT | 7 | 12 | 0.000 | 1.500 | true | false | true | false | false |
| R01 | 0x1 | REJECT | 2 | 1 | 0.000 | 0.000 | true | true | false | true | false |
| R02 | 0x2 | REJECT | 5 | 7 | 0.000 | 1.000 | true | true | false | false | false |
| R03 | 0x3 | REJECT | 10 | 19 | 0.000 | 0.750 | true | false | false | false | false |
| R04 | 0x4 | REJECT | 5 | 8 | 0.000 | 0.500 | true | false | false | false | false |
| R05 | 0x5 | REJECT | 4 | 5 | 0.000 | 0.000 | true | true | false | true | false |
| R06 | 0x6 | REJECT | 7 | 11 | 0.000 | 1.500 | true | false | true | false | false |
| R07 | 0x7 | REJECT | 5 | 9 | 0.000 | 2.000 | true | false | true | false | false |
| R08 | 0x8 | REJECT | 7 | 13 | 0.000 | 1.500 | true | true | true | false | false |
| R09 | 0x9 | REJECT | 4 | 7 | 0.000 | 1.000 | true | true | false | false | false |
| R10 | 0xa | REJECT | 6 | 11 | 0.000 | 1000000.000 | true | true | true | false | false |

## Budget hard caps

- States: 50000
- Edges: 500000
- CPU-s: 30

## Falsifier summary

- F1: `parallelism_ratio >= k_min > 1` (P4-falsifiable)
- F2: M1 Option 1 single-basic-block preemption-free step vs M2 boundary
- F3: Runtime domain witness with explicit losing sink

## GUARANTEES.md row 16 — checked-subset boundary

Send/close protocol ordering remains outside Tier-0 scope. This spike documents the boundary only.
