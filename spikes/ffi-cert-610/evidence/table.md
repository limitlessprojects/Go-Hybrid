| export | arity | own frame B | closure B | artifact class | forced by | Go route | signature | route |
|---|---|---|---|---|---|---|---|---|
| `hy_json_array_get` | 3 | 136 | 176 | DEEP | recursion, indirect-dispatch, missing-frame x25, external-callee x4 | DEEP | match | match |
| `hy_json_array_len` | 2 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | SMALL | match | too fast |
| `hy_json_array_push_string` | 3 | 88 | 128 | DEEP | recursion, indirect-dispatch, missing-frame x19, external-callee x6 | SMALL | match | too fast |
| `hy_json_get_array` | 4 | 120 | 784 | DEEP | recursion, indirect-dispatch, missing-frame x23, external-callee x5 | DEEP | match | match |
| `hy_json_get_int` | 4 | 88 | 752 | DEEP | recursion, indirect-dispatch, missing-frame x12, external-callee x4 | SMALL | match | too fast |
| `hy_json_get_string` | 4 | 104 | 768 | DEEP | recursion, indirect-dispatch, missing-frame x13, external-callee x5 | SMALL | match | too fast |
| `hy_json_live_count` | 1 | 40 | 48 | DEEP | recursion, indirect-dispatch, missing-frame x7, external-callee x5 | — | not consumed | — |
| `hy_json_new_array` | 1 | 56 | 80 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x5 | SMALL | match | too fast |
| `hy_json_new_object` | 1 | 56 | 80 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | SMALL | match | too fast |
| `hy_json_parse` | 3 | 200 | 224 | DEEP | recursion, indirect-dispatch, missing-frame x26, external-callee x7 | DEEP | match | match |
| `hy_json_release` | 1 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x4 | DEEP | match | match |
| `hy_json_set` | 4 | 184 | 208 | DEEP | recursion, indirect-dispatch, missing-frame x18, external-callee x5 | DEEP | match | match |
| `hy_json_to_string_pretty` | 4 | 136 | 176 | DEEP | recursion, indirect-dispatch, missing-frame x20, external-callee x6 | DEEP | match | match |
| `hy_json_value_text` | 4 | 120 | 144 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | — | not consumed | — |
| `hy_nop` | 1 | 0 | 0 | SMALL | — | RAW | drift 2→1 | match |
| `hy_regex_is_match` | 4 | 248 | 272 | DEEP | recursion, indirect-dispatch, missing-frame x26, external-callee x7 | DEEP | match | match |
| `hy_regex_is_valid` | 2 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | DEEP | match | match |
| `hy_sha3_digest256` | 3 | 776 | 800 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x5 | SMALL | match | too fast |
| `hy_sha3_stream_checksum` | 2 | 408 | 408 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x4 | SMALL | match | too fast |
| `hyrt_abi_word` | 0 | 0 | 0 | SMALL | — | DEFAULT | drift 1→0 | match |
| `hyrt_stack_alignment_probe` | 0 | 0 | 0 | SMALL | — | DEEP | drift 2→0 | match |

consumed: 19 | signature exact: 16/19 | route not faster than artifact: 11/19
