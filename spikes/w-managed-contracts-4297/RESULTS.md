# Spike Results: Verifiable `managed` Synchronization Contracts (Issue #4297)

## Summary
Empirical audit (M1) and compiler verification pass (M2) for Wildcard Track issue #4297:
`[wildcard] The managed hatch asserts an invariant nobody writes: make every escape name a checked synchronization contract, or delete it`.

## M1: Repo-Wide `managed` Block Audit & Contract Expressibility
Across the entire repository (`tests/checker-corpus/` and examples), only two `.hy` sources use `managed`:

| Source | Declared Category | Synchronization Invariant in Code | Contract Expressibility Verdict |
|--------|-------------------|-----------------------------------|---------------------------------|
| `tests/checker-corpus/18-pool-shared-struct-managed.hy:80` | `"mutex"` | `stats.mu` (`sync.mutex`) locks every `worker` update and `main` read of `stats.processed` / `stats.failures`. | **Expressible & Verified**: `guard: stats.mu, protects: stats` verifies cleanly (`verified: true`). |
| `tests/checker-corpus/02-shared-map-managed.hy:70` | `"mutex"` | Header claims `"shards tallied under the runner mutex"`, but **no mutex exists** (`counts[w]++` is an unsynchronized concurrent map write). | **Caught as False Claim**: Rejected under `--require-managed-contracts` (`HYB-E0005`). Adding `var mu sync.mutex` (`guard: mu, protects: counts`) makes it verify cleanly. |

**Unverifiable Perimeter Shrinkage**: Under `--require-managed-contracts`, uncontracted `managed` blocks shrink from **6 categories to 2** (`"interop"` and `"finalizer"`), while `"mutex"` and `"atomic"` require machine-checked contracts and `"lock_free"` / `"channel_handoff"` must migrate to checked primitives or channels/`move`.

## M2: Conformance & Falsifier Matrix (20/20 Passed)
| Test | Discipline | Expected | Result | Diagnostic |
|------|------------|----------|--------|------------|
| `m1_corpus_18_contracted_verifies` | `mutex` (`guard: stats.mu, protects: stats`) | Accept (`verified: 1`) | PASS | - |
| `m1_corpus_02_uncontracted_rejected_under_require_managed_contracts` | Uncontracted `"mutex"` under strict mode | Reject | PASS | `HYB-E0005` |
| `m1_corpus_02_fake_mutex_not_passed_to_worker_rejects` | `mutex` declared in `main`, omitted from `task` | Reject | PASS | `HYB-E0005` |
| `m1_corpus_02_synchronised_with_real_mutex_verifies` | `mutex` (`guard: mu, protects: counts`) | Accept (`verified: 1`) | PASS | - |
| `m2_mutex_reading_field_not_covered_by_contract_rejects` | Read `stats.unguarded` outside `protects: stats.guarded` | Reject | PASS | `HYB-E0005` |
| `m2_mutex_unlocked_read_in_main_rejects` | Unlocked read of `stats.processed` in `main` | Reject | PASS | `HYB-E0005` |
| `m2_mutex_unlocked_write_in_spawned_worker_rejects` | Write after `unlock()` in spawned `task` | Reject | PASS | `HYB-E0005` |
| `m2_mutex_wrong_lock_held_rejects` | Holding `mu_b` instead of `guard: mu_a` | Reject | PASS | `HYB-E0005` |
| `m2_mutex_passed_by_value_to_task_rejects` | `sync.mutex` copied by value across `task` | Reject | PASS | `HYB-E0005` |
| `m2_rw_mutex_rlock_permits_read_and_rejects_write` | `sync.rw_mutex` `rlock` read vs write | Accept / Reject | PASS | `HYB-E0005` |
| `m2_mutex_unbalanced_lock_across_branches_or_exit_rejects` | Missing `unlock()` on `if` branch | Reject | PASS | `HYB-E0005` |
| `m2_mutex_defer_unlock_in_worker_verifies_and_defer_in_loop_rejects` | `defer mu.unlock()` in fn vs loop | Accept / Reject | PASS | `HYB-E0005` |
| `m2_atomic_pkg_ops_verify` | `atomic.add_int64` / `atomic.load_int64` | Accept (`verified: 1`) | PASS | - |
| `m2_atomic_pointer_method_ops_verify` | `atomic.pointer[int]` `.store` / `.load` | Accept (`verified: 1`) | PASS | - |
| `m2_atomic_raw_read_or_write_rejects` | Raw `counter++` or `v := counter` under `"atomic"` | Reject | PASS | `HYB-E0005` |
| `m2_uncontracted_second_shared_root_inside_contracted_block_rejects` | Piggybacked unprotected map in contracted block | Reject | PASS | `HYB-E0005` |
| `m2_alias_laundering_of_protected_root_rejects` | `alias := counts` escaping critical section | Reject | PASS | `HYB-E0005` |
| `m2_outliving_task_unlocked_post_block_access_rejects` | Unlocked outer-root write after `managed` block | Reject | PASS | `HYB-E0005` |
| `m2_interop_and_finalizer_perimeter_category_rules` | `interop`/`finalizer` perimeter vs bogus contract | Accept / Reject | PASS | `HYB-E0005` |
| `m2_hy_diag_explain_e0005_exists` | `hyb explain HYB-E0005` reference page | Registered | PASS | - |
