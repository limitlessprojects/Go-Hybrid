# Spike 22 — Dual-Heap Allocation Certification (`noalloc`) and Zero-Alloc Bridge Lowering

**Associated Wildcard Proposal:** limitlessprojects/Go-Hybrid#394  
**Author:** @ocakciomursel-pixel  
**Target Milestone:** M1 Feasibility Spike & Pre-Registration  

---

## 1. Abstract

`MOONSHOT.md` Active Solicitation #4 asks for **heap-freedom proofs for the checked subset**.
In a dual-runtime language (Go GC + Rust system allocator), heap-freedom requires proving **both** that Go's compiler does not escape a value to the GC heap **and** that a Rust bridge crossing does not allocate on the native heap. Today, even a stack-only call like `doc.int("k")`, `sha3.digest256(buf)`, or `regex.is_match("^[a-z]+$", s)` performs 2 to 6 heap allocations per call across Go and Rust (`pinCrossKey`'s `[]byte(s)` copy + `runtime.Pinner` escape, `json.Marshal` + `serde_json::from_str` scalar boxing, and per-call `regex::Regex::new` compilation).

This spike pre-registers and delivers the concrete M1 verification artifact:
1. **Static Call-Graph Allocator & Stack Oracle (`alloc_cert_verifier.rs`)**: A standalone, dependency-free verification oracle that inspects all `hyrt` export symbols, frame budgets $W(f)$, recursion cycles, and `__rust_alloc` reachability.
2. **Three Zero-Alloc Bridge Lowerings**:
   - Pin-free `unsafe.StringData` borrow views for synchronous leaf crossings (`//go:noescape`).
   - Compile-time regex validation + hoisted automaton handle cache.
   - Typed scalar fast-paths (`hy_json_set_int`, `hy_json_set_bool`, `hy_json_set_str`).

---

## 2. Pre-Registered Falsification Criteria

- **Criterion F1 (Static Allocation Reachability Oracle):** The static call-graph reachability oracle must deterministically classify every `hy_*` export into `AllocFree` ($0$ heap allocations) or `MayAlloc`, proving that `hy_nop`, `hyrt_abi_word`, `hyrt_stack_alignment_probe`, `hy_sha3_digest256`, `hy_sha3_stream_checksum`, `hy_json_get_int`, and `hy_json_array_len` have zero paths to `__rust_alloc`.
- **Criterion F2 (Zero-Allocation Loop Invariant):** Across a 1,000-iteration benchmark loop, `testing.AllocsPerRun` on `sha3.digest256(buf)` and `doc.int("k")` must drop from $1\text{ alloc/op}$ to **exactly $0\text{ allocs/op}$**.
- **Criterion F3 (Honest Limits & Falsifiers):** Dynamic non-literal regex strings (`regex.is_match(user_pat, text)`) and unbounded JSON map parsing cannot be heap-free; the oracle must fail closed and reject them inside `noalloc` regions.
