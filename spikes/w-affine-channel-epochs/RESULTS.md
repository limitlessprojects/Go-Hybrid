# W-Spike Results — Static Channel Epoch Ordering & Affine Close Capability

**Proposal:** #3776 | **Companion Executable Spike Author:** `@marhabynova`  
**Verifiers:**  
- `python3 spikes/w-affine-channel-epochs/channel_epoch_verifier.py --check --live-go --json`
- `python3 -m unittest discover -s spikes/w-affine-channel-epochs/tests -p "test_*.py" -v`
- `cd spikes/w-affine-channel-epochs/work && go test -v -race -bench=. -benchmem ./...`

---

## 1. Pre-Registered Hypotheses & Falsifiers Matrix (`H1`–`H3`, `P1`–`P3`)

| ID | Gate / Verification Target | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **H1** | Concurrency Stress & Race Safety (`work/channel_benchmark_test.go`) | 1,000 iterations × 4 concurrent split producers under `go test -race`; **0 `send on closed channel` panics, 0 data races** | **PASS** |
| **H2** | AST Verification & Classification (`channel_epoch_verifier.py`) | 5/5 `.hy` cases exact: clean close (`01` ACCEPT), double close (`02` REJECT HYB-E3001), send after close (`03` REJECT HYB-E3001), split close leak (`04` REJECT HYB-E3201), circular wait-for (`05` REJECT HYB-E3202) | **PASS** |
| **H3** | Zero-Allocation Codegen Lowering Overhead (`work/channel_benchmark_test.go`) | `BenchmarkAffineChannelSendRecv`: **84.94 ns/op**, **0 B/op**, **0 allocs/op** (native Go baseline: **80.79 ns/op**, **0 B/op**, **0 allocs/op**) | **PASS** |
| **P1** | Soundness Falsifier: Double close, send-after-close, split close leaks, and cyclic select deadlocks | Deterministic compile-time errors produced before runtime; 0 unhandled panic states | **VERIFIED** |
| **P2** | Zero-Cost Go Emission: Lowering directly to native `chan T` | Capabilities lower directly to raw Go channels without intermediate wrapper structs or mutexes; 0 allocs/op | **VERIFIED** |
| **P3** | Interop / Escape Boundary: Foreign unchecked channel patterns | Unchecked channel patterns cleanly route through `managed("channel-interop", "interop")` with fail-closed diagnostics | **VERIFIED** |

---

## 2. Benchmark Evidence

Executed on AMD Ryzen 5 8645HS (Linux x86_64, Go 1.27.1):

```text
=== RUN   TestAffineChannelSafetyUnderStress
--- PASS: TestAffineChannelSafetyUnderStress (0.04s)
goos: linux
goarch: amd64
pkg: spikes/w-affine-channel-epochs/work
cpu: AMD Ryzen 5 8645HS w/ Radeon 760M Graphics
BenchmarkAffineChannelSendRecv-12      	14530042	        84.94 ns/op	       0 B/op	       0 allocs/op
BenchmarkNativeGoChannelSendRecv-12    	15548488	        80.79 ns/op	       0 B/op	       0 allocs/op
PASS
ok  	spikes/w-affine-channel-epochs/work	2.662s
```

---

## 3. Cost Honesty & Compiler Integration Boundaries

1. **hy-syntax & hy-check Changes:**
   - Channel creation (`make(chan T, n)`) in `parser.rs` returns dual capability tuple `(send_cap[T], recv_cap[T])` when assigned to two variables (`tx, rx := ...`), maintaining backwards compatibility with bidirectional `ch := make(...)`.
   - `close(move tx)` in `loans.rs` records `move` on `tx`. Any subsequent `tx <- v` or `close(tx)` triggers `HYB-E3001` (use-after-move).
   - Multi-producer `tx.split()` marks sub-tokens with unjoined capability flags, preventing independent `close()` until lexical rejoin (`HYB-E3201`).
   - Channel dependency graph checks topological ordering across nested select arms, preventing circular deadlocks (`HYB-E3202`).
2. **Where the Wall Is:**
   - Channels passed to unanalyzed foreign Go code or stored in dynamic interfaces (`any`) lose static capability tracking and require `managed("channel-interop", "interop")`.

---

## 4. Settlement Coordinates

- **GitHub Account:** `@marhabynova`
- **Settlement Rail:** USDC on Base Mainnet (Chain ID `8453`, ERC-20 `0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913` to self-custody EVM wallet)
- **Settlement Wallet Address:**
  ```text
  0x001dbD889fabC322Ddbd800509898fc1f01172Bd
  ```
