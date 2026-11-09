# Spike Results: W-026 (Issue #4200)

## Summary
Implementation and static verification of Milestone 1 for Wildcard Track issue #4200:
`[wildcard] Statically verifiable nursery cancellation and channel lifecycle invariants via structured effect types`.

## Verification Gates
- **P1: Linear SPSC Capability Split Syntax**: `hy.send_cap[T]` and `hy.recv_cap[T]` instantiated via `hy.pipe[T]()`.
- **P2: Affine Transfer Semantics**: `send_cap` and `recv_cap` move strictly into task closures. Affine use-after-move triggers `HYB-E3001`; reference capture across task boundaries (`&tx`, `&rx` or bare capture) triggers `HYB-E3002`.
- **P3: Nursery Exit Terminal Invariants**: Forward CFG dataflow analysis verifies all un-transferred `send_cap` handles reach `Closed` (`HYB-E4004`), and all un-transferred `recv_cap` handles reach `Drained` (`HYB-E4005`).
- **F1-F5**: Conformance matrix covering single, loop, select, pipeline, bidirectional, and nested nursery patterns.

## Conformance Matrix Results (20/20 Passed)
| Pattern | Description | Expected | Result | Diagnostic |
|---------|-------------|----------|--------|------------|
| P01 | Basic happy path (pipe, move tx, close in worker, drain in rx) | Accept | PASS | - |
| P02 | Undrained receiver at scope exit | Reject | PASS | HYB-E4005 |
| P03 | Unclosed sender at scope exit | Reject | PASS | HYB-E4004 |
| P04 | Affine use-after-move (send after move tx) | Reject | PASS | HYB-E3001 |
| P05 | Reference capture across task boundary (&tx) | Reject | PASS | HYB-E3002 |
| P06 | Multiple move of same capability | Reject | PASS | HYB-E3001 |
| P07 | Multi-stage pipeline (stage1 -> stage2) | Accept | PASS | - |
| P08 | Loop send with proper close and for-range drain | Accept | PASS | - |
| P09 | Unclosed sender in conditional block | Reject | PASS | HYB-E4004 |
| P10 | Select receive with drain | Accept | PASS | - |
| P11 | Unmoved cap capture across task boundary | Reject | PASS | HYB-E3002 |
| P12 | Move rx to consumer task and close tx in main | Accept | PASS | - |
| P13 | Drain rx via for-range loop | Accept | PASS | - |
| P14 | Use after move in select statement | Reject | PASS | HYB-E3001 |
| P15 | Explicit rx.drain() after worker completion | Accept | PASS | - |
| P16 | Bidirectional pipes cleanly split and moved | Accept | PASS | - |
| P17 | Nested nursery scopes with capability isolation | Accept | PASS | - |
| P18 | Unclosed sender in nested block | Reject | PASS | HYB-E4004 |
| P19 | Receiver use after move | Reject | PASS | HYB-E3001 |
| P20 | Multiple independent pipes in single scope | Accept | PASS | - |
