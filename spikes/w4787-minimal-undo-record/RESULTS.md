# Spike Results: W-4787 (Issue #4787)

## Summary
Milestone 1 reference model, combinatorial rank/unrank codec, and falsification test suite for Wildcard Track issue #4787:
`[wildcard] Give lossy task reductions a provably minimal undo record`.

The spike breaks the assumption that reductions losing information require raw input snapshots for replay. By emitting a within-group canonical rank of $\lceil \log_2 \binom{n}{k} \rceil$ witness bits alongside the retained reduction count, task inputs can be safely released and perfectly reconstructed on demand during debug replay.

## Verification Gates
- **G1: Exact Reversible Reconstruction**: All $2^8 = 256$ ordered 8-flag inputs round-trip with zero reconstruction error using their within-group combinatorial rank.
- **G2: Provably Minimal Witness Bound**: Witness bits match the information-theoretic lower bound $\lceil \log_2 \binom{n}{k} \rceil$ (0 bits for $k=0, 8$, 3 bits for $k=1, 7$, 7 bits for $k=4$).
- **G3: Falsification of Out-of-Bounds Ranks**: Ranks outside $[0, \binom{n}{k})$ are rejected with diagnostic `HYB-E8001: Rank out of bounds`.
- **G4: Storage Advantage**: For all counts $k \in [0, 8]$, the emitted undo witness consumes fewer bits than a raw 8-bit input snapshot.

## Conformance Matrix Results (5/5 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_exhaustive_256_roundtrip_recovery` | All 256 8-bit flag patterns | 100% exact recovery | PASS |
| `test_theoretical_witness_bits_bounds` | Information-theoretic bit counts | Match $\lceil \log_2 \binom{n}{k} \rceil$ | PASS |
| `test_falsifier_out_of_bounds_rank_rejected_with_e8001` | Out-of-bounds rank $\ge \binom{n}{k}$ | Rejection with `HYB-E8001` | PASS |
| `test_negative_rank_rejected` | Negative rank rejection | Rejection with `HYB-E8001` | PASS |
| `test_bit_savings_over_raw_snapshot` | Witness size vs raw snapshot | $\le 7$ bits (vs 8 bits) | PASS |
