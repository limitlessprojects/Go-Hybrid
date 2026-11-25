# W-1427 Spike Results — Kill O8 Non-Lexical Map-Slot Loans via Write-Through Exclusive Loans

**Proposal:** #1427 | **Author:** `@hamzy-hams` | **Verifiers:** `pytest spikes/w1427-write-through-slot-loans -v` & `python3 spikes/w1427-write-through-slot-loans/write_through_slot_loans.py --check`

## Executive Summary

Go-Hybrid lowers O8 map-slot loans (`n := &m[k]`) to a local value copy (`n := m[k]`) paired with a single deferred store-back (`m[k] = n`) placed at the inferred last lexical mention `last_use[n]` via a ~450-line control-flow state machine in `compiler/crates/hy-emit/src/stmt.rs` (`loan_frames`, `branch_inherit`, `flushed_before`, `flush_loans_for_return`, `flush_loans_for_loop`, and synthetic `else` injection). Because `m[k]` and local `n` diverge between the mutation site and `last_use[n]`, three structural bug classes arise:
1. **Class A (Causal Inversion):** Adding or removing a pure read (`_ := n.bal` / `fmt.println(n.bal)`) shifts `last_use[n]` and retroactively changes the value observed by earlier map reads (`10` vs `99`).
2. **Class B (Zero-Trip Loop Store-Back Drop):** When a `for` loop is the last user of `n` and mentions `m`, `branch_inherit` moves `m[k] = n` inside the loop body and marks `flushed_before` in the enclosing scope. When the loop runs `0` iterations (`0..0`), `m[k] = n` is permanently dropped even before `task worker(move m, done)` transfers ownership.
3. **Class C (Interprocedural Clobber on Pure Read):** A pure read `last_read := n.bal` at the end of `n`'s lexical window emits `m[k] = n`, overwriting intervening mutations (`500 -> 99`) and breaking `m[k] == *n`.

**Write-Through Exclusive Loans** replaces deferred `last_use[n]` store-back with:
- **Emitter rule (~35 lines replacing ~450 lines in `hy-emit/src/stmt.rs`):** Lower `n := &m[k]` to `hyKey := k; n := m[hyKey]` and emit `m[hyKey] = n` **immediately after every statement that mutates a binding rooted at `n`** (`n.field = v`, `n.field op= v`, `n.field++`, `n = v`). Pure reads never emit map writes; `return`, `break`, `continue`, `?`, and `for` require zero flush hooks.
- **Checker rule (~25 lines in `hy-check/src/loans.rs`):** Enforce `n := &m[k]` as a true exclusive borrow across its non-lexical `[created, last_use[n]]` window (`HYB-E3102` on any direct or interprocedural map read/write during the active window, and `HYB-E3006` on `const` map slot loans).

---

## 1. Pre-Registered Hypotheses & Verification Matrix (`P1`–`P4`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **P1** | **Causal-Inversion Elimination (`Class A`, `hy/01_class_a_causal_inversion.hy`)** | Deferred lowering flips `read_bal(m)` from `99` (without trailing `n.bal` read) to `10` (with trailing `n.bal` read); Write-Through deterministically observes `99` in both cases and Exclusive A3 rejects map reads inside `[created, last_use]` with `HYB-E3102` | **PASS** |
| **P2** | **Zero-Trip Loop Store-Back Soundness (`Class B`, `hy/02_class_b_zero_trip_loop_drop.hy`)** | On `for i in 0..0` before `task worker(move m, done)`, deferred `branch_inherit` + `flushed_before` drops `n.bal = 99` (`worker_saw_bal == 10`, `ok == false`); Write-Through stores `m[hyKey] = n` immediately at `n.bal = 99` (`worker_saw_bal == 99`, `ok == true`) | **PASS** |
| **P3** | **Interprocedural Clobber Elimination & Statement-Boundary Invariant (`Class C`, `hy/03_class_c_interproc_loan_clobber.hy`)** | Deferred flush on trailing pure read `last_read := n.bal` clobbers `500` back to `99`; Write-Through emits zero store-backs on pure reads, maintains `m[k] == *n` at every statement boundary, and rejects interprocedural slot-loan conflicts with `HYB-E3102` | **PASS** |
| **P4** | **Corpus Migration Accounting (`130` files) & Orthogonal Composability (`#1432`, `#1441`, `#1645`/`#2331`)** | `11` slot-loan corpus fixtures (`6 REJECT`, `5 ACCEPT`) scanned across all `130` files in `tests/checker-corpus/`: **`0 / 130` verdict flips**, **`0` diagnostic code changes**, `-1` spurious store-back on read-only loan (`23-a1c-mapslot-copy-at-spawn.hy`), and 100% composability with `#1432` / `#1441` (`HYB-E3104`) and `#1645` / `#2331` (`HYB-E3102` / `HYB-E3006`) | **PASS** |

---

## 2. Corpus Migration Accounting (`tests/checker-corpus/` — `130` Programs)

| Corpus Fixture | Pre-Registered Verdict | Verdict Under Write-Through | Deferred Store-Backs | Write-Through Store-Backs | Emission / Migration Delta |
| :--- | :--- | :--- | :---: | :---: | :--- |
| `06-borrow-returned-from-fn.hy` | `REJECT LOAN-ESCAPE` (`HYB-E3104`) | `REJECT LOAN-ESCAPE` (`HYB-E3104`) | 0 | 0 | **None** (`0` changes) |
| `08-a3-map-slot-invalidated.hy` | `REJECT A3` (`HYB-E3102`) | `REJECT A3` (`HYB-E3102`) | 0 | 0 | **None** (`0` changes) |
| `22-a1c-mapslot-loan-at-spawn.hy` | `REJECT A1c` (`HYB-E3001`) | `REJECT A1c` (`HYB-E3001`) | 0 | 0 | **None** (`0` changes) |
| `37-a3-interproc-window.hy` | `REJECT A3` (`HYB-E3102`) | `REJECT A3` (`HYB-E3102`) | 0 | 0 | **None** (`0` changes) |
| `42-a3-assign-map-slot-invalidated.hy` | `REJECT A3` (`HYB-E3102`) | `REJECT A3` (`HYB-E3102`) | 0 | 0 | **None** (`0` changes) |
| `map-slot-borrow-callarg-reject.hy` | `REJECT LOAN-ESCAPE` (`HYB-E3104`) | `REJECT LOAN-ESCAPE` (`HYB-E3104`) | 0 | 0 | **None** (`0` changes) |
| `62-shadowing-binder-last-use.hy` | `ACCEPT` | `ACCEPT` | 1 | 1 | **Identical Go**; removes `#547` `for_binder_shadows` lookahead |
| `63-shadowing-binders-at-spawn.hy` | `ACCEPT` | `ACCEPT` | 1 | 1 | **Identical Go**; removes `#547` `if`-init / closure shadow lookahead |
| `map-slot-borrow-paths-accept.hy` | `ACCEPT` (`3 4 7`) | `ACCEPT` (`3 4 7`) | 2 | 2 | **Identical Go** and identical output `3 4 7` |
| `23-a1c-mapslot-copy-at-spawn.hy` | `ACCEPT` (`frontend: 5`) | `ACCEPT` (`frontend: 5`) | 1 | **0** | **Eliminates 1 spurious map write** on a read-only loan |
| `09-a3-map-slot-valid.hy` | `ACCEPT` (`services: 3`) | `ACCEPT` (`services: 3`) | 1 | **2** | **+1 map write** for per-statement coherence; verdict & output unchanged |

---

## 3. Orthogonal Composability with `#1432`, `#1441`, and `#1645`/`#2331`

| Layer | Rule / Code | Mutual Invariant Provided |
| :--- | :--- | :--- |
| **`#1441` (Creation-Site Position Guard)** | `HYB-E3104` (`O2` / `O8`) | Rejects `&m[k]` in `var` declarations, call arguments, mixed parallel assignments, and `any` struct literals—guaranteeing to the Write-Through emitter that every accepted `&m[k]` is bound 1:1 to a plain local identifier `n`. |
| **`#1432` (Target-Identity & Rebind Guard)** | `HYB-E3104` (`O8` / `T24`) | Rejects rebinding a live slot loan `n` via multi-return calls (`n, err = f()`), closure captures, or pointer rebinds while tracking whole-value updates (`n = v`)—allowing Write-Through to immediately emit `n = v; m[hyKey] = n`. |
| **`#1645` / `#2331` (Interprocedural & Closure Slot-Loan Guard)** | `HYB-E3102` (`A3`) & `HYB-E3006` (`T22`) | Extends `block_mutates_param`, `expr_fnlit_mutates_param`, and `collect_mutation_roots` to detect callee/closure `&m[k]` loans and `m[k].field = v` writes—closing checker-side interprocedural A3 blindness and `const` map data races. |
| **`#1427` (Write-Through Exclusive Loans)** | `HYB-E3102` (`A3` / `O8` / `T24`) | Emits `m[hyKey] = n` immediately after each mutation of `n` (never on pure reads) and makes `n`'s `[created, last_use]` window exclusive—eliminating `loan_frames` / `branch_inherit` and maintaining `m[k] == *n` at every statement boundary. |

---

## 4. Why Write-Through Beats a Lexical Entry Block (`with &m[k] as n { ... }`)

1. **Preserves Corpus #09 (`09-a3-map-slot-valid.hy`):** In `scale_then_register`, `svc := &e.services[existing]` is mutated on lines 53–54 and `e.services[fresh] = ...` is inserted on line 55 **in the same block**. Corpus #09 explicitly forbids block-scoped loan windows (`"a checker that rejects ANY slot-borrow near a map mutation (block-scoped windows instead of non-lexical ones) fails this program"`). Write-Through keeps the non-lexical `[created, last_use]` window in `hy-check` so both Corpus #08 (`REJECT A3`) and Corpus #09 (`ACCEPT`) pass unchanged.
2. **Eliminates Intra-Block Buffering & Exit Hooks:** A lexical `with &m[k] as n { ... }` block that flushes at `}` still buffers dirty state inside the block across early `return`, `break`, `continue`, `?`, and helper calls. Write-Through keeps `m[hyKey] == *n` after every semicolon with **zero** control-flow exit hooks.
3. **Zero Spurious Writes on Read-Only Loans (`23-a1c-mapslot-copy-at-spawn.hy`):** Flushing at block exit unconditionally writes `m[hyKey] = n` even when `n` was only read, whereas Write-Through emits `0` map writes on read-only loans.

---

## 5. Before / After `GUARANTEES.md` Semantics

| `GUARANTEES.md` Row / Rule | Code | Current Semantics (`Deferred at last_use[n]`) | Semantics Under Write-Through Exclusive Loans (`#1427`) |
| :--- | :--- | :--- | :--- |
| **Row 3 (`A3` / `O8` — No container conflict during element loan)** | `HYB-E3102` | `n := &m[k]` rejects direct/interproc map mutations during `[created, last_use[n]]`, but allows map reads (`m[k]`, `read_bal(m)`) that observe stale pre-mutation state before `last_use[n]`. | `n := &m[k]` is an **exclusive borrow** across `[created, last_use[n]]`: any direct or interprocedural read or mutation of `m` while `n` has remaining uses is rejected with `HYB-E3102`. |
| **Row 6 (`T24` / `O8` — Map-slot loan lowering & no address-of on loan)** | `HYB-E3104` | Lowered to `n := m[k]` plus a deferred `m[k] = n` at `last_use[n]` via `loan_frames` / `branch_inherit`. Pure reads shift when `m[k] = n` executes; 0-trip `for` handoffs drop `m[k] = n`. | Lowered to `hyKey := k; n := m[hyKey]` with `m[hyKey] = n` emitted **immediately after each mutation rooted at `n`**. Pure reads never emit map writes; control-flow exits need no flush hooks. |
| **Row 1 (`A1b` / `A1c` — Move freeze & spawn/send sendability)** | `HYB-E3001` / `HYB-E3003` | Relies on `stmt_has_boundary_using` / `flushed_before` lookahead before `task worker(move m)`, which drops updates when a 0-trip loop intervenes (`Class B`). | Every mutation `n.f = v` prior to `task worker(move m)` has already written through to `m[hyKey]` on the mutation statement itself. |
| **Row 4 (`A4` / `O9` — Go-heap reads are copies)** | `O9` | Read-only slot loans (`23-a1c-mapslot-copy-at-spawn.hy`) still emit a deferred `m[k] = n` store-back at `last_use[n]`. | Read-only slot loans emit **zero** store-backs, matching `O9` in both checker and emitter. |
| **`T22` (`IMMUT` — Deep `const` immutability)** | `HYB-E3006` | Direct `const` map writes are rejected; interprocedural/closure `&m[k]` borrows required `#1645`/`#2331`/`#2054` guards. | Composes with `#1645`/`#2331`: taking `&m[k]` on a `const` map (directly, interprocedurally, or in a closure) is rejected with `HYB-E3006`. |

---

## 6. Reproducing Verification Locally

```bash
pytest spikes/w1427-write-through-slot-loans -v
python3 spikes/w1427-write-through-slot-loans/write_through_slot_loans.py --check
```
