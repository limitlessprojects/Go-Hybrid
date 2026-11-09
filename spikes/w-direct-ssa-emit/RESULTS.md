# W-SSA verification matrix and test results

All 15 tests pass with `python -m unittest discover -s spikes/w-direct-ssa-emit/tests -v`.

| Test | Area | Condition tested | Result |
|---|---|---|---|
| test_01 | Arithmetic | Basic block instruction ordering | pass |
| test_02 | Types | Float64, string, and boolean type handling | pass |
| test_03 | Soundness | Owned places receive stack certificates | pass |
| test_04 | Falsifier M1 | Escaped address revokes stack certificate | pass |
| test_05 | Control flow | Branching CFG with phi node placement | pass |
| test_06 | Falsifier M2 | Phi type mismatch raises error | pass |
| test_07 | Loops | Loop header, body, back-edge, and phi values | pass |
| test_08 | Concurrency | Channel make, send, and receive lowering | pass |
| test_09 | Memory | Struct field address and load projections | pass |
| test_10 | Keywords | Go reserved words cause no naming conflicts | pass |
| test_11 | Row 13 | Unused variables produce no compile errors | pass |
| test_12 | SSA rules | Unique value IDs preserved across assignments | pass |
| test_13 | Dominance | Operands defined before use in block order | pass |
| test_14 | Resolution | Undefined variables fail closed | pass |
| test_15 | Expressions | Nested binary operations lower cleanly | pass |

Suite execution time: 0.001s.
