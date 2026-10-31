# Wildcard #1669 Spike: Structured Task Scopes & Affine Cancellation Tokens

- **Issue:** Closes #1669 (Wildcard Track `💎 Bounty` + `wildcard`)
- **Payout address (USDC on Base / EVM):** `0x0879d4Cc6Ce06596eC04dBa5F5c86e45285D28C2`

## 1. M1 Formal Specification (`scope { ... }` & Affine `JoinHandle[T]`)

- **Surface syntax:** `scope { h := task f(x); ...; join(move h) | cancel(move h) | drain(move h) }`
- **Typing & Affine Containment Rule (`HYB-E3501`):** Every `task` spawned inside a lexical `scope` binds an affine `JoinHandle[T]` in state `UNJOINED`. On every exit edge of `scope` (normal block end, `return`, `break`, `continue`, `select` timeout), all live handles must be in state `CONSUMED`; otherwise `HYB-E3501 (unjoined task escape)` is emitted.
- **Double Consumption (`HYB-E3502`):** Consuming a handle twice emits `HYB-E3502 (use-after-join of affine handle)`.
- **Foreign Callback Boundary (`HYB-E3503`):** Spawning inside a foreign Go callback (`http.HandleFunc`) without an enclosing `managed("...", "foreign_callback")` permit fails closed with `HYB-E3503`.
- **Zero-Allocation Lowering:** Verified scopes lower to stack-inlined `var __hy_wg sync.WaitGroup` + `defer __hy_wg.Done()` + `__hy_wg.Wait()`.

## 2. Reproduction

```bash
python3 -m unittest discover -s spikes/w1669-structured-task-scopes/tests -p "test_*.py" -v
```
