# W-020 (#417) Spike: Serializable Shared-State Regions via Admitted Transactions (MVSI)

## 1. Summary & Ledger Reference
- **Issue:** `#417` (`[wildcard] Serializable shared-state regions instead of exclusive ownership`)
- **Ledger ID:** `W-020` (Slot 20, `$250` M1 + `$250` M2 = `$500` total)
- **Architecture:** Opt-in transactional state region (`stm_region`) over immutable versioned scalar roots. Tasks share cell identities and execute effect-restricted transactions with private write buffering, full read-set version validation under a single commit lock, and atomic root publication.

## 2. M1 Verification Matrix (7 Scenarios + 3 Negative Protocol Controls + Effect Admission)
| Scenario / Control | Full Read-Set Validation (`stm_region.py`) | Negative Control (`write_only` / `stale_base` / `no_ryw`) |
|---|---|---|
| **S1–S7 (Lost Update, Write Skew, Transfers, Blind Writes, RYW, Abort, Read-Only)** | `0` serial-oracle violations across all interleaved schedules | Write-skew violates invariant `(a+b >= 1)` under `write_only` (`68/70` schedules fail) |
| **Merge writes into old snapshot (`stale_base`)** | Preserves concurrent disjoint writes (`(1, 1)`) | Overwrites peer's committed cell (`18/20` schedules fail) |
| **Ignore staged writes on read (`no_ryw`)** | Observes private staged write (`read == 2`) | Returns stale snapshot value (`70/70` schedules fail) |
| **Effect Admission Gate (`check_tx_admission`)** | Rejects I/O (`HYB-E6001`), channels (`HYB-E6002`), `task` (`HYB-E6003`), foreign/nested/mutable-capture (`HYB-E6004`) | Fail-closed on unknown callees |
