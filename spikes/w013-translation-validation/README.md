# W-013 Feasibility Spike: Translation Validation of Lowered Go Against a Checked-Effect IR (#345)

- **Issue:** Closes #345 (Wildcard Track **W-013**, Slot 13 in `MOONSHOT.md` / Discussion #365)
- **Proposal credit:** `@MyTH-zyxeon`
- **Payout address (USDC on Base / EVM):** `0x0879d4Cc6Ce06596eC04dBa5F5c86e45285D28C2`

## 1. Pre-Registered Scope & Questions Pressed in Maintainer Triage

Per maintainer triage on #345, this bounded falsification spike evaluates three concrete questions without touching compiler crates:
1. **Checked-Effect IR Specification (`CheckedEffectIR`):** Explicit sequence of observable effect and ownership-boundary operations (`STORE_BACK`, `EFFECT_CALL`, `MOVE_SEND`, `MATCH_RELEASE`, `MANAGED_MARKER`, `EXIT`) bound by SHA-256 digest (`ir_digest`, `go_bytes_digest`, `effect_summary_digest`, `validator_version`).
2. **Validator Trusted Computing Base (TCB) vs `hy-emit`:** Independent Go CFG parser and forward-simulation verifier (`148 LOC` TCB vs `612 LOC` in `hy-emit` = **`0.242x` TCB ratio**, sharing zero code with `hy-emit`).
3. **Falsification & Per-Function Cost:** Rejects **6/6** Go-type-correct non-equivalent mutations (`M1_drop_store_back`, `M2_reorder_store_back_across_call`, `M3_duplicate_effect`, `M4_redirect_exit`, `M5_unauthorized_managed`, `M6_unmodeled_panic`) at **`< 0.05 ms/fn`**.

## 2. Reproduction

```bash
python3 -m unittest discover -s spikes/w013-translation-validation/tests -p "test_*.py" -v
```
