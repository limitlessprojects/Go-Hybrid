# Spike Results: W-4634 (Issue #4634)

## Summary
Milestone 1 reference model, integral conservation validator, and disjoint write partitioning engine for Wildcard Track issue #4634:
`[wildcard] Let a transfer plan carry both the write partition and the conservation law`.

The spike breaks the assumption that scientific kernel task partitioning and physical invariants must be expressed separately. A geometric transfer plan simultaneously defines disjoint destination write rows for workers and strictly preserves field integrals across arbitrary non-conforming 1D meshes.

## Verification Gates
- **G1: Rational Conservation Exactness**: Integral of density fields is preserved exactly ($I_{src} == I_{dst}$) without numerical float drift.
- **G2: Oracle Agreement**: Across 1,536 exhaustive mesh and density configurations on $[0, 4]$ with 4,608 worker layouts, the partitioned execution matches the raster oracle with zero errors.
- **G3: Falsifier Rejection**: Swapped equal-width plans (preserving total but scrambling cell locations) are rejected by `validate_plan`.
- **G4: Disjoint Worker Slices**: Each worker independently writes strictly disjoint rows without inter-thread contention or locks.

## Conformance Matrix Results (3/3 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_density_example_preserves_integral` | [4, 1] over [0, 1, 3] -> [0, 2, 3] | Integral 6 preserved | PASS |
| `test_falsifier_swapped_plan_rejected` | Swapped spatial rows | Rejected | PASS |
| `test_exhaustive_mesh_combinations` | 1,536 cases across [0, 4] cuts | 100% Oracle Match | PASS |
