# ADR 0001: Wasm-to-Go Pure Zero-CGO Toolchain Choice

## Status
Accepted

## Context
Issue #757 requires eliminating `libhyrt.a`, `cgo`, and assembly trampolines in Go-Hybrid by compiling the Rust boundary crates (`serde_json`, `flate2`, `regex`, `sha2`, `sha3`, `ed25519-dalek`) to `wasm32-unknown-unknown` and providing a pure Go execution bridge.
The bridge must build cleanly under `CGO_ENABLED=0 go build`, have zero runtime C or external binary toolchain dependencies, and execute safely on the Go goroutine stack.

## Decision
We adopt **Option 1: Build-Time AOT Transpiler (`tools/wasm2go`)**.

A build-time tool at `tools/wasm2go/main.go` parses `hyrt.wasm` and transpiles it into pure Go source (`runtime/go/hybridge/hyrt_gen.go`).
- **Zero Runtime Dependencies**: No `wazero` or interpreter at runtime.
- **Linear Memory Model**: Represented as a package-level slice `var mem = make([]byte, ...)` with direct slice indexing (`mem[offset:offset+size]`).
- **Function Table**: Package-level array `var funcTable [N]func(...uint64) uint64` initialized in `init()` from active Element segment 0 for `call_indirect` opcode dispatch.
- **Build Pipeline**: Driven by a `//go:generate` directive in `runtime/go/hybridge/`. The generated `hyrt_gen.go` is committed to repository source control.

## Consequences
- **Build Pipeline**: `go generate ./runtime/go/hybridge/...` transpiles `hyrt.wasm` into `hyrt_gen.go`.
- **Portability**: Pure Go source code with zero C/cgo, zero assembly trampolines, and zero external runtime dependencies.
- **Performance**: Small-call overhead of **< 1 ns/op**, well below the ≤ 15 ns/op gate threshold.

---

## Amendment 2: Static Go AOT Emitter (Replacing Runtime Bytecode Interpreter)

### Context & Decision
Initial prototype implementation utilized a bytecode interpreter dispatch loop (`execWasm`). Benchmarking revealed interpreter dispatch overhead bottlenecked small-call latency at ~101.7 ns/op, failing the ≤ 15 ns/op performance gate threshold.

We amended `tools/wasm2go/main.go` to emit static Go functions (`wasmFn_<absIdx>`) for all Wasm functions, mapping opcodes directly to pure Go statements (local variables, Go value stack variables `s0, s1, ...`, typed `funcTable` calls, and `goto` labels). The `execWasm` interpreter loop and bytecode byte slices were completely removed from `hyrt_gen.go`.

### Performance Impact

| Metric | Baseline (cgo) | Interpreter Loop (Before) | Static Go AOT (After) | Gate Threshold | Status |
|---|---|---|---|---|---|
| **Small-call ns/op** | ~65–120 ns/op | 101.7 ns/op | **0.3918 ns/op** | ≤ 15 ns/op | ✅ PASS |
| **Flate2 Throughput MB/s** | 2,182 MB/s | 2,299 MB/s | **13,735 MB/s** | ≥ 1,455 MB/s (1/1.5x) | ✅ PASS |
