# Spike Results: W-4729 (Issue #4729)

## Summary
Milestone 1 reference model, BFS state-space search engine, and downstream `.hy` client synthesizer for Wildcard Track issue #4729:
`[wildcard] Make an ownership-breaking release produce its smallest downstream client`.

The spike breaks the baseline assumption in `GUARANTEES.md` (Rows 6 & 7) that the unit of an ownership check is an isolated program rather than a pair of API releases. By exploring the state space of legal client call sequences over an owned value across two revisions, the synthesizer produces a concrete, minimal `.hy` witness that passes against the old release and fails against the candidate release with diagnostic `HYB-E3001: use of moved value`.

## Verification Gates
- **G1: Shortest Witness Synthesis**: When an API operation changes its ownership effect from `borrow` to `consume`, the synthesizer discovers the minimal breaking sequence (e.g. `[Submit, Inspect]`) via BFS.
- **G2: Directional Asymmetry**: Transitioning from `consume` to `borrow` (relaxation) produces 0 breaking witnesses and reports `COMPATIBLE`.
- **G3: Invariance Under Benign Changes**: Identical signatures and renamed local variables report 0 witnesses without false alarms.
- **G4: Cost Honesty & Aliasing Bounds**: Unsupported features such as complex pointer aliasing are explicitly flagged as `UNKNOWN` rather than emitting false claims.
- **G5: Replay Fidelity**: Synthesized `.hy` programs pass cleanly against $v_1$ and fail precisely with `HYB-E3001` against $v_2$.

## Conformance Matrix Results (5/5 Passed)
| Test Case | Description | Expected Status / Diagnostic | Result |
|:---|:---|:---:|:---:|
| `test_borrow_to_consume_breaking_witness_synthesized` | `Submit` changes from borrow to consume | `WITNESS_FOUND` / `HYB-E3001` on v2 | PASS |
| `test_consume_to_borrow_relaxation_produces_no_breaking_witness` | `Submit` relaxes from consume to borrow | `COMPATIBLE` (0 witnesses) | PASS |
| `test_identical_api_with_renamed_locals_gives_no_witness` | Identical API signatures and contracts | `COMPATIBLE` (0 witnesses) | PASS |
| `test_unsupported_aliasing_reports_unknown` | API operation with aliased pointer | `UNKNOWN` (bound contract) | PASS |
| `test_enumeration_falsifier_matches_model` | Exhaustive search on two-operation package | `WITNESS_FOUND` at length 2 | PASS |
