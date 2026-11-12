# W-SSA verification matrix and test results

All 25 unit tests pass with `python -m unittest discover -s spikes/w-direct-ssa-emit/tests -v`.

### Unit Test Matrix

| Test | Area | Condition Tested | Result |
|---|---|---|---|
| test_01 | Arithmetic | Basic block instruction ordering | pass |
| test_02 | Types | Float64, string, and boolean type handling | pass |
| test_03 | Soundness | Owned places receive stack certificates | pass |
| test_04 | Gate 2 | Escaped address revokes stack certificate | pass |
| test_05 | Control flow | Branching CFG with phi node placement | pass |
| test_06 | Soundness | Phi type mismatch fails closed | pass |
| test_07 | Loops | Loop header, body, back-edge, and phi values | pass |
| test_08 | Concurrency | Channel make, send, and receive lowering | pass |
| test_09 | Memory | Struct field address and load projections | pass |
| test_10 | Row 13 | Go reserved words cause no naming conflicts | pass |
| test_11 | Row 13 | Unused variables produce no compile errors | pass |
| test_12 | SSA rules | Unique value IDs preserved across assignments | pass |
| test_13 | Dominance | Operands defined before use in block order | pass |
| test_14 | Resolution | Undefined variables fail closed | pass |
| test_15 | Expressions | Nested binary operations lower cleanly | pass |
| test_16 | Gate 2 | GC stack frame pointer bitmap generation | pass |
| test_17 | Gate 2 | Write barrier insertion under active GC mark | pass |
| test_18 | Gate 2 | Forced GC stress simulation safepoints | pass |
| test_19 | Gate 2 | Cross-task channel escape certificate revocation | pass |
| test_20 | Gate 1 | Toolchain adapter version compatibility across 1.21, 1.22, 1.26.9 | pass |
| test_21 | Gate 1 | Internal opcode remapping absorption | pass |
| test_22 | Edge cases | Defer unwinding and SSA return sequencing | pass |
| test_23 | Edge cases | Nonblocking select with default case phi joins | pass |
| test_24 | Edge cases | Nested loops with labeled break and continue | pass |
| test_25 | Gate 3 | Struct field partial stack certification and write barriers | pass |

### Benchmark Results

From `spikes/w-direct-ssa-emit/benchmarks/benchmark_ssa.py` (20,000 iterations):

- Direct SSA Lowering: 22.28 microseconds / function
- Stack Allocated Places: 80,000 / 80,000 (100% stack allocation rate for proven places)
- Heap Allocated Escapes: 0
- Active Write Barriers Inserted: 20,000
- GC Stack Frame Bitmap: `0b10000` (live pointer tracking)
- Suite execution time: 0.004s
