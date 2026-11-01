# ffi-cert-610: Proof-Carrying FFI Boundary Contracts

Spike for **[#610](https://github.com/limitlessprojects/Go-Hybrid/issues/610)**: *"Make FFI claims proof-carrying: verify boundary contracts against the linked Rust artifact"*.

## 1. Defensive Architecture & Fail-Closed Invariants
Replaces hand-curated FFI descriptors (`interfaces/descriptors.md`) with contracts mechanically extracted from the built `libhyrt.a` ELF archive and verified against Go bindings.

- **Stack Budget:** Missing frame records, dynamic dispatch, or recursion cycle -> fail-closed degradation to `DEEP`.
- **Pointer Retention:** Retained pointer (`Box::into_raw`, `Box::leak`) -> handles-only crossing (`HYB-E7003`).
- **Callbacks:** Indirect call under `may_callback=false` -> reject (`HYB-E7004`).
- **Blocking:** Blocking primitives beneath non-blocking export -> reject (`HYB-E7005`).
- **Signatures & Hash:** Mismatches between binding and artifact -> build abort (`HYB-E7001`, `HYB-E7006`).

## 2. Milestone 1 Results: Falsification Suite
6/6 pre-registered negative fixtures fail closed:
- `F1` (`HYB-E7001`): Parameter arity drift -> **REJECTED**
- `F2` (`HYB-E7002`): Recursion claimed as `SMALL` -> **REJECTED**
- `F3` (`HYB-E7003`): Retained borrow (`Box::leak`) -> **REJECTED**
- `F4` (`HYB-E7004`): Indirect call with `may_callback=false` -> **REJECTED**
- `F5` (`HYB-E7005`): Blocking callee (`Mutex`) beneath non-blocking -> **REJECTED**
- `F6` (`HYB-E7006`): Mismatched embedded artifact hash -> **REJECTED**

## 3. Milestone 2 Results: Trust-Boundary Learning
18 of 21 exports collapse to `DEEP` due to:
1. Prebuilt stdlib (`std`/`core`/`compiler_builtins`) lacking `.stack_sizes` frame records.
2. Recursion cycles in allocation error paths (`__rust_dealloc` <-> `__rdl_alloc`).
3. GOT-indirect dispatch preventing bounded stack proof.

The verifier soundly revokes unprovable fast-path privileges, ensuring memory and scheduler safety.

## 4. Execution
