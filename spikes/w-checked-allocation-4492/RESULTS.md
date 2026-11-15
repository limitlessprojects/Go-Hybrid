# Spike Results: W-028 (Issue #4492)

## Summary
Implementation and static verification of Milestone 1 for Wildcard Track issue #4492:
`[wildcard] Checked allocation: a heap budget for the checked subset`.

## Proposal Overview
Currently, `GUARANTEES.md` Row 17 states memory safety on the Go side is `"Go's GC, unchanged"`, leaving resource consumption and heap growth unverified. Issue #4492 introduces **Checked Allocation**:
1. **Compositional Budget Vocabulary**: Functions carry static allocation summaries in one of three canonical forms:
   - `Constant(bytes)`: Bounded allocation known at compile-time (e.g. fixed struct escapes, sized buffers).
   - `Linear(input, factor)`: Scaled allocation bounded as a function of input parameter length (e.g. `len(items) * sizeof(T)`).
   - `Unbounded`: Dynamic, unprovable allocation requiring explicit budget attribution or isolation within `managed("...", "interop")`.
2. **Fail-Closed Diagnostics**: Any unbudgeted, dynamic allocation site inside the checked subset triggers compile-time rejection:
   - `HYB-E5001`: Unbounded heap allocation without budget bound.
   - `HYB-E5002`: Unmetered string concatenation allocation hazard.
   - `HYB-E5003`: Recursive allocation without static recursion ceiling.
   - `HYB-E5004`: Declared allocation budget exceeded by internal construct.

## Conformance Matrix Results (20/20 Passed)
| Pattern | Description | Expected | Result | Diagnostic | Budget Class |
|---------|-------------|----------|--------|------------|--------------|
| P01 | Primitive stack-allocated bindings with zero heap escape | Accept | PASS | - | CONSTANT |
| P02 | Escaping struct literal with fixed scalar fields within budget | Accept | PASS | - | CONSTANT |
| P03 | Fixed-size make slice with constant capacity | Accept | PASS | - | CONSTANT |
| P04 | Make slice with data-dependent runtime length without budget bound | Reject | PASS | HYB-E5001 | UNBOUNDED |
| P05 | Append in unmetered loop without pre-allocated capacity | Reject | PASS | HYB-E5001 | UNBOUNDED |
| P06 | Append within pre-sized slice using capacity window | Accept | PASS | - | CONSTANT |
| P07 | Map literal with dynamic runtime insertion in loop | Reject | PASS | HYB-E5001 | UNBOUNDED |
| P08 | String concatenation in loop with quadratic allocation hazard | Reject | PASS | HYB-E5002 | UNBOUNDED |
| P09 | Format string interpolation with fixed scalar arguments | Accept | PASS | - | CONSTANT |
| P10 | Interface boxing of value type with proven constant overhead | Accept | PASS | - | CONSTANT |
| P11 | Closure capturing mutable state escaping to foreign scope | Reject | PASS | HYB-E5001 | UNBOUNDED |
| P12 | Recursive function without static allocation ceiling summary | Reject | PASS | HYB-E5003 | UNBOUNDED |
| P13 | Explicit constant budget attribute on bounded struct factory | Accept | PASS | - | CONSTANT |
| P14 | Declared budget exceeded by nested composite literal | Reject | PASS | HYB-E5004 | CONSTANT |
| P15 | Linear budget summary parameterized on named input slice length | Accept | PASS | - | LINEAR |
| P16 | Compositional budget deduction across non-recursive call hierarchy | Accept | PASS | - | CONSTANT |
| P17 | Unchecked dynamic allocation isolated inside explicit managed block | Accept-Managed | PASS | - | UNBOUNDED |
| P18 | Channel send of pre-allocated moved buffer with zero net allocation | Accept | PASS | - | CONSTANT |
| P19 | Multi-branch control flow with one unbudgeted allocation path | Reject | PASS | HYB-E5001 | UNBOUNDED |
| P20 | Checked hot loop verified with zero allocation per operation | Accept | PASS | - | CONSTANT |

## Verification Command
```bash
python spikes/w-checked-allocation-4492/alloc_budget_verifier.py
python -m unittest discover -s spikes/w-checked-allocation-4492/tests
```
