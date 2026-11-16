# Spike Results: W-029 (Issue #4488)

## Summary
Implementation and static verification of Milestone 1 for Wildcard Track issue #4488:
`[wildcard] Publication-authority regions: checked parallel intent plans with aggregate obligations`.

## Architecture & Formal Model
Publication-authority regions solve the fundamental problem that race-free execution can still publish an invalid effect plan if independently permitted tasks collectively exceed global resource bounds.

### Core Constructs:
1. **Immutable Task Grants**: Tasks receive static grants `Grant(worker_id, resource, op, allowance)` defining identity, permitted targets, and individual ceilings.
2. **Typed Intents**: Parallel workers produce pure declarative `Intent(worker_id, resource, op, units)` rather than direct external effects.
3. **Aggregate Obligation Validation**: The join barrier evaluates the cumulative batch against global capacity limits before publication (`HYB-E6001`).
4. **Canonical Encoding**: All execution permutations of valid worker intents sort into a byte-identical canonical plan.
5. **Transitive Purity**: Intent generation blocks foreign I/O, clocks, random inputs, and shared mutable memory (`HYB-E6009`).

## Conformance Matrix Results (20/20 Passed)
| Pattern | Description | Expected | Result | Diagnostic |
|---------|-------------|----------|--------|------------|
| P01 | Permutation [W1, W2, W3] within global cap 10 | Accept | PASS | - |
| P02 | Permutation [W1, W3, W2] within global cap 10 | Accept | PASS | - |
| P03 | Permutation [W2, W1, W3] within global cap 10 | Accept | PASS | - |
| P04 | Permutation [W2, W3, W1] within global cap 10 | Accept | PASS | - |
| P05 | Permutation [W3, W1, W2] within global cap 10 | Accept | PASS | - |
| P06 | Permutation [W3, W2, W1] within global cap 10 | Accept | PASS | - |
| P07 | Aggregate cross-task obligation exceeds global cap (10 > 9) | Reject | PASS | HYB-E6001 |
| P08 | Intent targets wrong resource unauthorized by grant | Reject | PASS | HYB-E6002 |
| P09 | Duplicate worker identities in parallel authority region | Reject | PASS | HYB-E6003 |
| P10 | Missing intent from designated mandatory authority worker | Reject | PASS | HYB-E6004 |
| P11 | Intent units exceed individual worker allowance | Reject | PASS | HYB-E6005 |
| P12 | Intent specifies zero or negative obligation units | Reject | PASS | HYB-E6006 |
| P13 | Intent submitted by undeclared foreign worker identity | Reject | PASS | HYB-E6007 |
| P14 | Duplicate resource intent per worker | Reject | PASS | HYB-E6008 |
| P15 | Transitive purity violation (clock/random access in intent region) | Reject | PASS | HYB-E6009 |
| P16 | Disjoint multi-resource publication batch | Accept | PASS | - |
| P17 | All-or-nothing publication rollback on partial worker error | Reject | PASS | HYB-E6005 |
| P18 | Deterministic canonical sorting by (worker_id, resource, op) | Accept | PASS | - |
| P19 | Hierarchical child authority region nested in parent barrier | Accept | PASS | - |
| P20 | Full multi-worker parallel publication plan verified race-free | Accept | PASS | - |

## Invariant Proof
- **Permutation Invariance**: Verified across all 6 permutations of 3 workers with identical canonical plan digest.
- **Fail-Closed Gate**: 8 distinct negative invariants tested and proven fail-closed with assigned diagnostic codes.
