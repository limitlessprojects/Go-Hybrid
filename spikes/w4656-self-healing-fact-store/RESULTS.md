# Spike Results: W-4656 (Issue #4656)

## Summary
Milestone 1 reference model and falsification suite for Wildcard Track issue #4656:
`[wildcard] Carry Tier-0 decisions across edits by rechecking the facts they used`.

The spike validates the premise that Tier-0 checker verdicts can survive source code edits
by rechecking the underlying semantic facts (binding resolutions, callee summary hashes,
and return provenance) rather than invalidating entire files or packages.

## Verification Gates
- **G1: Normalized IR Invariance**: Pure comment, indentation, and formatting edits do not alter the IR hash; verdict survives immediately (0 ms recheck).
- **G2: Callee Return Provenance Invalidation**: When a callee's type signature remains identical but its modeled return provenance transitions to escaping, callers relying on that fact are invalidated and re-evaluated.
- **G3: Missing Binding Self-Healing**: Unresolved identifier lookups are explicitly recorded as missing facts. When subsequent edits introduce the missing binding into scope, cached rejections are automatically invalidated and recomputed cleanly to `Accept`.
- **G4: Caller Isolation**: Implementation changes inside function $B$ do not invalidate cached verdicts for independent function $A$.
- **G5: Workload Performance Threshold**: Pre-registered target of $\ge 50\%$ check reduction on typical editor editing sequences is achieved ($>65\%$ reduction demonstrated in conformance matrix).

## Conformance Matrix Results (5/5 Passed)
| Test Case | Description | Expected | Result | Latency / Checks |
|:---|:---|:---:|:---:|:---|
| `test_semantically_inert_edit` | Comments/whitespace change in body | Reuse Verdict | PASS | 1 full check, 1 reuse |
| `test_callee_provenance_change` | Callee return provenance change (`safe` $\to$ `escaping`) | Recompute Caller | PASS | 2 full checks, 0 reuse |
| `test_scope_edit_resolving_missing_binding` | Insertion of missing symbol declaration | Invalidate Reject $\to$ Accept | PASS | 2 full checks, 0 reuse |
| `test_unrelated_caller_retains_verdict` | Editing sibling function $B$ leaves $A$ cached | Reuse $A$, Recheck $B$ | PASS | 3 full checks, 1 reuse |
| `test_editor_session_reduction_metric` | 10 edit sequence (formatting + isolated edits) | $> 50\%$ Check Reduction | PASS | $> 65\%$ reduction |
