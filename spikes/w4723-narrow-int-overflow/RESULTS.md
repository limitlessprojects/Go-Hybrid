# Spike Results: W-4723 (Issue #4723)

## Summary
Reference model and fix validation for issue #4723:
`hy-check: constant conversions to narrower integer types fold out-of-range constants (int8(200) accepted, prints -56) instead of rejecting like Go`.

The current `hy-check` only applies the E6006 overflow rule at 64-bit width, silently folding out-of-range constants for narrower integer targets (`int8`, `int16`, `uint8`, etc.). This spike validates that the overflow rule must be applied uniformly across all integer widths to match Go's strict constant rules.

## Verification Gates
- **G1: Narrow Integer Overflow Rejection**: `int8(200)`, `int8(-200)`, `uint8(256)`, `int16(32768)` are all correctly rejected with `E6006: constant N overflows T`.
- **G2: Boundary Values Accepted**: `int8(-128)`, `int8(127)`, `uint8(255)`, `int64(MAX)` are correctly accepted.
- **G3: Float Truncation Rejection**: `int8(200.9)` is rejected as not representable (truncated), matching Go.
- **G4: Full Differential Table Match**: All 11 cases from the issue's differential test table match expected Go compiler behavior.

## Conformance Matrix Results (9/9 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_int8_200_rejected_like_go` | `int8(200)` overflow | `E6006` rejected | PASS |
| `test_int8_neg200_rejected_like_go` | `int8(-200)` overflow | `E6006` rejected | PASS |
| `test_int8_boundary_values_accepted` | `int8(-128)` and `int8(127)` | Accepted | PASS |
| `test_uint8_256_rejected` | `uint8(256)` overflow | `E6006` rejected | PASS |
| `test_uint64_negative_rejected` | `uint64(-1)` overflow | `E6006` rejected | PASS |
| `test_int64_max_accepted` | `int64(9223372036854775807)` | Accepted | PASS |
| `test_int16_32768_rejected` | `int16(32768)` overflow | `E6006` rejected | PASS |
| `test_float_truncation_rejected` | `int8(200.9)` truncation | `E6006` rejected | PASS |
| `test_full_conversion_table_matches_go` | All 11 cases from issue table | Match Go | PASS |
