# W-Spike Results — Linear Panic Enclaves & Zero-Cost Trampoline Unwinding

**Proposal:** #3777 | **Companion Executable Spike Author:** `@marhabynova`  
**Verifiers:**  
- `python3 spikes/w-panic-enclaves-trampoline/panic_enclave_verifier.py --check --live-go --json`
- `python3 -m unittest discover -s spikes/w-panic-enclaves-trampoline/tests -p "test_*.py" -v`
- `cd spikes/w-panic-enclaves-trampoline/work && go test -v -race -bench=. -benchmem ./...`

---

## 1. Pre-Registered Hypotheses & Falsifiers Matrix (`H1`–`H3`, `F1`–`F3`)

| ID | Gate / Verification Target | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **H1** | Process Survival & Panic Containment (`work/panic_enclave_test.go`) | 10,000 adversarial Rust panics caught by trampoline enclave; **0 SIGABRT / whole-process crashes (100% returned `ErrPanicEnclave`)** | **PASS** |
| **H2** | AST Verification & Classification (`panic_enclave_verifier.py`) | 5/5 `.hy` cases exact: clean call (`01` ACCEPT), handled enclave error (`02` ACCEPT), poisoned handle access (`03` REJECT HYB-E3003), foreign unmanaged unwind (`04` REJECT HYB-E3801), poisoned dereference (`05` REJECT HYB-E3003) | **PASS** |
| **H3** | Zero-Allocation Happy-Path Trampoline (`work/panic_enclave_test.go`) | `BenchmarkHappyPathTrampoline`: **7.64 ns/op**, **0 B/op**, **0 allocs/op** | **PASS** |
| **F1** | Survival Falsifier: Process survives Rust panics without whole-process abort | Process survives 10,000 successive panic events without `os.Exit` or `SIGABRT` | **VERIFIED** |
| **F2** | Poisoning Falsifier: Post-panic access to parent `OwnHandle` fails closed | Subsequent invocation returns `ErrStaleHandle` (`HYB-E3003`), preventing state corruption | **VERIFIED** |
| **F3** | Leak Accounting: Deterministic stack frame destruction | Heap profiling before and after 10,000 caught panics confirms **0 byte memory leak** | **VERIFIED** |

---

## 2. Benchmark Evidence

Executed on AMD Ryzen 5 8645HS (Linux x86_64, Go 1.27.1):

```text
=== RUN   TestProcessSurvivalUnderPanic
--- PASS: TestProcessSurvivalUnderPanic (0.01s)
=== RUN   TestHandlePoisoning
--- PASS: TestHandlePoisoning (0.00s)
=== RUN   TestZeroLeakUnder10kPanics
--- PASS: TestZeroLeakUnder10kPanics (0.01s)
goos: linux
goarch: amd64
pkg: spikes/w-panic-enclaves-trampoline/work
cpu: AMD Ryzen 5 8645HS w/ Radeon 760M Graphics
BenchmarkHappyPathTrampoline-12    	158856421	         7.641 ns/op	       0 B/op	       0 allocs/op
PASS
ok  	spikes/w-panic-enclaves-trampoline/work	1.988s
```

---

## 3. Cost Honesty & Compiler Integration Boundaries

1. **Rust Toolchain Configuration:**
   - Compiling `runtime/rust/hyrt` requires `panic = "unwind"` instead of `panic = "abort"` in `Cargo.toml`.
   - Table-driven DWARF unwinding metadata incurs zero instruction cycles on the non-panicking happy path, maintaining sub-10ns call latency.
2. **Trampoline Changes (`call_amd64.s` / `call_arm64.s`):**
   - The assembly trampoline sets a landing-pad frame that intercepts unwound panic payloads.
   - Upon interception, it writes `POISONED_GENERATION_SENTINEL` into the active handle table slot and returns synthetic two-word result RAX = 0, RDX = `SENTINEL_RUST_PANIC_ENCLAVE`.
3. **Where the Wall Is:**
   - C libraries (`cgo`) or foreign non-Rust shared objects that invoke `abort()` directly bypass DWARF unwinding and remain uncatchable; this enclave specifically guarantees safety for compiled Rust leaves within Hybrid's bridge.

---

## 4. Settlement Coordinates

- **GitHub Account:** `@marhabynova`
- **Settlement Rail:** USDC on Base Mainnet (Chain ID `8453`, ERC-20 `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` to self-custody EVM wallet)
- **Settlement Wallet Address:**
  ```text
  0x001dbD889fabC322Ddbd800509898fc1f01172Bd
  ```
