# Spike Results: W-4781 (Issue #4781)

## Summary
Milestone 1 reference model, DAG maximum-weight closure solver, and witness schedule generator for Wildcard Track issue #4781:
`[wildcard] Certify peak live buffer bytes across schedules and return a schedule that reaches the bound`.

The spike breaks the assumption that memory budget analysis requires fixing a runtime scheduler beforehand. By representing buffer acquisitions ($+$) and releases ($-$) as weighted events in a precedence DAG, every permitted interleaving is analyzed via predecessor-closed sets (maximum-weight closure), yielding the exact peak live payload bytes and a concrete legal witness schedule that attains that bound.

## Verification Gates
- **G1: Exact Peak Bound Certification**: Mathematical peak live payload bytes match the maximum closure across all legal execution interleavings.
- **G2: Proposal Model Reproducibility**: Reproduces the author's exact 6-event setup with 100% precision: 10 legal orders, 4 peaking at 8 bytes, 6 peaking at 13 bytes, with certified peak = 13 bytes.
- **G3: Disjoint Overlap Independence**: Disjoint concurrent buffers correctly certify the sum of their capacities ($10 + 20 = 30$), while serialized buffers take the maximum ($\max(10, 20) = 20$).
- **G4: Witness Trace Realizability**: Returns a verified, topologically valid schedule prefix that attains the certified peak without violating precedence constraints.

## Conformance Matrix Results (4/4 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_author_six_event_proposal_example` | 6-event DAG (A=8, B=5, C=3) | 10 orders (4 @ 8, 6 @ 13) | PASS |
| `test_independent_concurrent_buffers_sum_peaks` | Independent buffers A(10) + B(20) | Peak = 30 bytes | PASS |
| `test_strictly_serialized_buffers_take_maximum_not_sum` | Serialized A(10) $\to$ B(20) | Peak = 20 bytes | PASS |
| `test_zero_capacity_buffer_neutral_impact` | Buffer with capacity 0 | Peak = 15 bytes | PASS |
