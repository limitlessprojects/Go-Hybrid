# Wildcard Spike (#1755): Zero-Trampoline SSA Register-ABI Bridge & Linear Place-Capability Tokens

**Issue:** [#1755](https://github.com/limitlessprojects/Go-Hybrid/issues/1755)  
**Track:** Wildcard / Moonshot Track (Prompts 2 & 3 Combined Deletion)

## 1. Summary of Architectural Deletion

This executable spike validates the core claim of [#1755](https://github.com/limitlessprojects/Go-Hybrid/issues/1755):
1. **Prompt 3 Replaced ($O(|V| + |E|)$ Linear Place-Capability Tokens):** Instead of constructing a path-sensitive inter-procedural alias graph over raw pointers, mutable slices carry linear SSA capability tokens (`Perm.UNIQUE`, `Perm.SHARED`, `Perm.FROZEN`). Disjoint range splits (`s[0:mid]`, `s[mid:len]`), linear writes, and CFG `phi` merges are checked in a single linear pass over the SSA graph.
2. **Prompt 2 Replaced (Zero-Trampoline Register-ABI Bridge):** Because every slice descriptor `(ptr, len, cap)` in `RAX, RBX, RCX` is paired with a statically verified capability token and leaf-frame stack headroom bound, cross-core calls require **0 C-ABI staticlib glue instructions**, **0 runtime bounds checks inside inner loops**, and **0 `runtime.morestack` prologues** when stack headroom suffices.

## 2. Verification & Reproduction

Run the unit test suite directly with standard library `unittest`:

```bash
python -m unittest discover -s spikes/zero_trampoline_ssa_place_capabilities/tests -p "test_*.py" -v
```

All 4 verification invariants pass with zero external dependencies:
- `test_linear_unique_write_and_double_use_rejection`
- `test_disjoint_slice_split_and_rejoin`
- `test_ssa_phi_conservation`
- `test_zero_trampoline_register_abi_call`
