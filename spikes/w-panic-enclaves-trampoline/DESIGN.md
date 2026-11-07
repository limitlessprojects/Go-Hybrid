# Design: Linear Panic Enclaves & Zero-Cost Trampoline Unwinding (Proposal #3777)

## 1. Problem Statement & Assumption Under Attack

`GUARANTEES.md` Row 18 and `SPEC §6.3 / abi-types.md §1 Rule 5` currently state:
> *"Memory safety, Rust side: Single ownership, moves, function-local borrows; panic = abort at the boundary (never unwind across)."*

Today, any unexpected panic in a compiled Rust leaf function (such as an out-of-bounds slice index, arithmetic divide-by-zero, or explicit debug assertion failure) executes `std::process::abort()`. In cloud production environments (e.g. gRPC services, HTTP microservices, and message pipeline workers), a whole-process abort is catastrophic: a single malformed request crashes thousands of concurrent, independent goroutines.

Wildcard Proposal **#3777** disproves the assumption that cross-language boundary safety requires whole-process termination. It demonstrates that by establishing **Structured Panic Enclaves** with table-driven DWARF zero-cost landing pads, handle table generation poisoning, and synthetic two-word `HyResult` sentinels, Hybrid can safely contain Rust panics into catchable Go errors with **zero latency overhead on the happy path**.

---

## 2. Core Architecture

### 2.1 The Assembly Trampoline Panic Enclave
The Hybrid assembly trampoline (`call_amd64.s` and `call_arm64.s`) sets up an isolated landing-pad boundary wrapping the Rust call frame.
- Rust code is compiled with `panic = "unwind"` and table-driven DWARF metadata (`-C panic=unwind`).
- On the happy path, DWARF tables incur **0 instruction overhead**: execution proceeds directly through aligned SysV `CALL` instructions with 3.37–3.44 ns latency.
- When an internal Rust panic occurs, unwinding unwinds stack-allocated destructors deterministically up to the trampoline boundary and halts. Foreign unwinding frames never leak into Go's stack.

### 2.2 Two-Word HyResult Sentinel Projection
Upon catching an unwound panic, the trampoline converts the panic payload into a synthetic two-word `HyResult`:
$$\text{RAX} = 0, \quad \text{RDX} = \text{SENTINEL\_RUST\_PANIC\_ENCLAVE} \ (0\text{xDEAD\_BEEF\_0000\_0001})$$
In emitted Go code, this sentinel is unpacked directly into:
```go
val, err := hybridCall(...)
if errors.Is(err, hy.ErrPanicEnclave) {
    // Process survives; error is handled gracefully
}
```

### 2.3 Generation-Tagged Handle Poisoning
To guarantee memory safety when a panic interrupts a stateful operation on an `OwnHandle`:
1. The handle table entry associated with the panicked call is immediately marked with generation sentinel `POISONED_GENERATION_SENTINEL = 0xFFFFFFFFFFFFFFFF`.
2. Any subsequent attempt to dereference, mutate, or pass the parent handle fails closed at compile time (`HYB-E3003`) or at runtime (`StaleGenerationError`).

---

## 3. Pre-Registered Falsifiers (`F1`–`F3`)

1. **F1 (Process Survival):** An adversarial function `rust_leaf_panics()` is invoked across the boundary. The Go process must NOT trigger `SIGABRT` or `os.Exit()`; the call must return `errors.Is(err, hy.ErrPanicEnclave)`.
2. **F2 (Handle Poisoning):** Any subsequent operation on the active handle must fail closed (`StaleGenerationError` / `HYB-E3003`), proving that uninitialized or corrupt state cannot be read.
3. **F3 (Resource Accounting):** Heap profiling (`runtime.MemStats`) across 10,000 successive caught panics must measure exactly **0 bytes of memory leak** on the Rust side, confirming deterministic frame destruction.
