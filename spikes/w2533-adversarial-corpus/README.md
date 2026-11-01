# Spike W-2533: Self-Farming Adversarial Corpus Engine

**Milestone:** M1 (Acceptance) & M2 (Learning)  
**Issue Reference:** #2533  
**Status:** Complete Spike & Tooling Implementation  
**Settlement Coordinates:** `0x8366bCe3a2D379Dec7656D7A67015789FaF999f20` (Base L2 USDC)

---

## 1. Executive Summary

This spike implements the self-farming adversarial corpus generator proposed in issue #2533. Instead of hand-authoring edge cases one hypothesis at a time, the engine systematically mutates known-accepted `.hy` programs from `tests/checker-corpus/` using 8 structure-aware mutation operators, subjecting them to dual-oracle verification (`hyb check` -> `hyb build --emit-go`).

Any mutation that escapes diagnostics (Row 13 BUILDABLE violation) or causes a compiler panic/ICE (P0 bug) is automatically isolated, reduced, and formatted into an adversarial reproduction report.

---

## 2. Architecture & Design Invariants

```
 +------------------------+      +---------------------------+
 | tests/checker-corpus/  | ---> |  MutationEngine (W-2533)  |
 | (130+ accepted cases)  |      |  - Loop var mutations     |
 +------------------------+      |  - Break alias leaks      |
                                 |  - Type & arity shifts    |
                                 +---------------------------+
                                               |
                                               v
                                 +---------------------------+
                                 |   Dual-Oracle Verifier    |
                                 |   hyb check vs go build   |
                                 +---------------------------+
                                               |
                     +-------------------------+-------------------------+
                     |                                                   |
                     v                                                   v
        [ Clean Reject: HYB-Exxxx ]                           [ Verdict Flip: Alert ]
        - Logged in operator coverage                         - Minimized reproduction
        - Confirms diagnostic barrier                         - Emitted Go race / build fail
```

### Deterministic Seed & Zero Dependencies
- Implemented in pure Python 3 standard library (`tools/adversarial_fuzzer.py`), strictly adhering to runner discipline (SPEC §2).
- Deterministic seeding via `--seed <int>` allows 100% reproducible mutation traces across GitHub Actions shards.

---

## 3. Mutation Operators & Coverage Matrix

| Operator | Target Diagnostic | Invariant Tested | Description |
| :--- | :--- | :--- | :--- |
| `op_loop_var_mutation` | `HYB-E3003` / `E3004` | Loop induction | Mutates induction variable in single-iteration loop (#2512) |
| `op_break_scope_leak` | `HYB-E3006` / Freeze | Outer join | Mutates break statement exit states across loops (#2511) |
| `op_float_int_mismatch` | `HYB-E1002` | Type soundness | Replaces float64 with typed int variables (#2551, #2528) |
| `op_make_cap_non_int` | `HYB-E2005` | Built-in arity | Injects non-integer expressions into make() len/cap (#2540) |
| `op_copy_slice_mismatch` | `HYB-E1002` | Built-in copy | Copies mismatched element types in slices (#2538) |
| `op_non_nilable_nil` | `HYB-E1002` | Nil equality | Compares non-nilable primitives to nil (#2471) |
| `op_untyped_int_delete_key` | `HYB-E2005` | Map delete | Injects untyped int constants as float map keys (#2536) |
| `op_match_arm_conflict` | `HYB-E1002` | Pattern match | Injects literal match patterns conflicting with subject (#2534) |

---

## 4. Verification & Testing

Unit tests for the mutation engine and evaluator pass 100%:

```bash
cd tools && python3 -m unittest test_adversarial_fuzzer.py
```

Run mutation farm:
```bash
python3 tools/adversarial_fuzzer.py --corpus-dir tests/checker-corpus --iterations 20 --report spikes/w2533-adversarial-corpus/evidence.md
```

---

## 5. Settlement Coordinates
- **Network:** Base L2
- **Asset:** USDC
- **Recipient:** `0x8366bCe3a2D379Dec7656D7A67015789FaF999f20`
