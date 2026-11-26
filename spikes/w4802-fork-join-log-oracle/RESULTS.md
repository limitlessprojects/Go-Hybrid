# Spike Results: W-4802 (Issue #4802)

## Summary
Milestone 1 reference model, DAG precedence compiler, and memoized process-mining alignment oracle for Wildcard Track issue #4802:
`[wildcard] Compile fork/join blocks into an oracle for incomplete event logs`.

The spike breaks the assumption that checked concurrent blocks only emit executable Go code. By compiling straight-line fork/join blocks into an event precedence graph, an operator can query incomplete, noisy, or dropped logs post-mortem to determine the minimal edit distance explaining the log, legal execution traces, and whether records suffer from task attribution ambiguity.

## Verification Gates
- **G1: Optimal Edit Distance**: Memoized search over `(completed_events_mask, log_offset)` determines the exact minimal number of missed or extra events required to explain an observed log.
- **G2: Oracle Cross-Agreement**: Output distances agree 100% with an independent exhaustive topological oracle across linear, diamond, and multi-worker fork/join graphs.
- **G3: Attribution Ambiguity Detection**: When multiple optimal interleavings attribute an observed event to different tasks, `has_attribution_ambiguity` is flagged to prevent false blame.
- **G4: Falsifier Rejection**: Inverted or chronologically impossible traces (such as `[done, enter]`) are rejected as 0-distance and correctly penalized.

## Conformance Matrix Results (4/4 Passed)
| Test Case | Description | Expected | Result |
|:---|:---|:---:|:---:|
| `test_two_workers_exact_log_has_attribution_ambiguity` | Symmetric 2-worker fork/join trace | Distance 0, Ambiguity=True | PASS |
| `test_falsifier_inverted_log_needs_two_missed_records` | Reversed log `[done, enter]` | Distance 2 | PASS |
| `test_empty_log_equals_number_of_missed_events` | Empty observed log | Distance = N | PASS |
| `test_independent_topological_oracle_agreement` | Multi-case cross-validation with topological oracle | 100% Agreement | PASS |
