# RESULTS.md — 20-region empirical results

**Spike**: `spikes/w3900-progress-strategies`
**Issue**: `limitlessprojects/Go-Hybrid#3900`
**Generated**: 2026-11-08T15:47:00Z
**Verdict anchor**: `VERDICT_2026-11-08T05:40Z` (WO-2)
**k_min**: 1.5

## 20-region empirical results table

| Region | Seed | Verdict | States | Edges | CPU-s | Ratio | Rank-OK | ChClose-OK | F1 | F2 | F3 |
|--------|------|---------|--------|-------|-------|-------|---------|------------|----|----|----|
| N01 | 0x81 | REJECT | 6 | 5 | 0.000 | 0.000 | false | true | false | true | true |
| N02 | 0x82 | REJECT | 6 | 5 | 0.000 | 0.000 | true | true | false | true | true |
| N03 | 0x83 | REJECT | 6 | 5 | 0.000 | 0.000 | true | false | false | true | true |
| N04 | 0x84 | REJECT | 6 | 5 | 0.000 | 0.000 | false | true | false | true | true |
| N05 | 0x85 | REJECT | 6 | 5 | 0.000 | 0.000 | true | false | false | true | true |
| N06 | 0x86 | UNKNOWN | 6 | 5 | 0.000 | 0.000 | false | true | false | true | true |
| N07 | 0x87 | UNKNOWN | 50000 | 50000 | 0.000 | 0.000 | false | true | false | true | true |
| N08 | 0x88 | REJECT | 7 | 6 | 0.000 | 0.000 | true | true | false | true | false |
| N09 | 0x89 | REJECT | 6 | 6 | 0.000 | 0.250 | true | true | false | false | true |
| N10 | 0x8a | REJECT | 7 | 6 | 0.000 | 0.000 | true | true | false | true | false |
| R01 | 0x01 | ADMIT | 7 | 12 | 0.000 | 1000000.000 | true | true | true | true | true |
| R02 | 0x02 | ADMIT | 8 | 14 | 0.000 | 1000000.000 | true | true | true | true | true |
| R03 | 0x03 | ADMIT | 9 | 16 | 0.000 | 1000000.000 | true | true | true | true | true |
| R04 | 0x04 | ADMIT | 10 | 18 | 0.000 | 1000000.000 | true | true | true | true | true |
| R05 | 0x05 | ADMIT | 6 | 10 | 0.000 | 1000000.000 | true | true | true | true | true |
| R06 | 0x06 | ADMIT | 6 | 8 | 0.000 | 1.500 | true | true | true | true | true |
| R07 | 0x07 | ADMIT | 8 | 14 | 0.000 | 1000000.000 | true | true | true | true | true |
| R08 | 0x08 | ADMIT | 9 | 16 | 0.000 | 1000000.000 | true | true | true | true | true |
| R09 | 0x09 | ADMIT | 10 | 18 | 0.000 | 1000000.000 | true | true | true | true | true |
| R10 | 0x0a | ADMIT | 6 | 10 | 0.000 | 1000000.000 | true | true | true | true | true |

**Verdict summary: ADMIT=10 REJECT=8 UNKNOWN=2 (total=20)**

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
