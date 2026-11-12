# W-2533 Adversarial Corpus Mutation Engine: Evidence Report

## Operator Coverage Matrix

| Operator | Tested | Accepted | Clean Rejected | P0 Flips |
| :--- | :--- | :--- | :--- | :--- |
| `op_loop_var_mutation` | 3 | 0 | 3 | 0 |
| `op_break_scope_leak` | 6 | 0 | 6 | 0 |
| `op_float_int_mismatch` | 3 | 0 | 3 | 0 |
| `op_make_cap_non_int` | 4 | 0 | 4 | 0 |
| `op_copy_slice_mismatch` | 1 | 0 | 1 | 0 |
| `op_non_nilable_nil` | 3 | 0 | 3 | 0 |
| `op_untyped_int_delete_key` | 0 | 0 | 0 | 0 |
| `op_match_arm_conflict` | 0 | 0 | 0 | 0 |

Total runs: 20 | Flips detected: 0
