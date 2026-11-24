# Spike Results: W-4617 (Issue #4617)

## Summary
Milestone 1 reference model and exact floating-point summation engine for Wildcard Track issue #4617:
`[wildcard] Let worker counts change without changing the bits of a floating-point sum`.

The spike breaks the assumption that a source loop commits the program to its intermediate roundings. By introducing opaque `ExactAccumulator` partials using integer-scaled unit arithmetic (mantissa << 1074), the compiler can freely change chunk sizes and merge tree shapes without altering a single bit of the final `binary64` result.

## Verification Gates
- **G1: Worker-Count Independence**: Cancellation, subnormal, and wide-exponent datasets produce bit-identical results across 1, 2, 3, 7, and 16 workers.
- **G2: Falsification of Ordinary Fold**: Ordinary left-fold `float64` summation disagrees with the rational oracle on the same cancellation data, confirming that intermediate rounding destroys bits.
- **G3: Nonfinite Input Rejection**: `NaN` and `Inf` inputs are rejected with diagnostic `HYB-E7002: nonfinite input rejected from exact summation region`.
- **G4: Affine Accumulator Reuse Rejection**: Attempting to reuse a finalized accumulator is rejected with `HYB-E7001: accumulator already finalized; affine reuse forbidden`.
- **G5: Signed Zero Correctness**: Exact zero from cancellation maps to `+0.0`, never `-0.0`.

## Conformance Matrix Results (7/7 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_cancellation_dataset_exact_across_worker_counts` | [1e16, 1, -1e16]*31 across 5 worker counts | Bit-identical | PASS |
| `test_subnormal_dataset_exact_across_worker_counts` | Subnormal [ulp, ulp, -ulp]*31 | Bit-identical | PASS |
| `test_wide_exponent_range_exact` | [2^900, -2^900, 2^-60, -0.0]*23 | Bit-identical | PASS |
| `test_ordinary_fold_disagrees_with_exact` | Ordinary left-fold vs oracle | Disagrees | PASS |
| `test_nonfinite_input_rejected_with_e7002` | NaN and Inf rejection | `HYB-E7002` | PASS |
| `test_finalized_accumulator_reuse_rejected_with_e7001` | Affine reuse after finalize | `HYB-E7001` | PASS |
| `test_exact_zero_maps_to_positive_zero` | 1.0 + (-1.0) = +0.0 | +0.0 | PASS |
