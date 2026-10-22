# Issue #757 Performance Optimization Designs — Option 1 AOT Transpiler

This document presents the 6-tier call-stack trace and architecture for the `tools/wasm2go` build-time AOT transpiler.

---

## 6-Tier Call-Stack Trace (AOT Static Emission)

```
Go-Hybrid
├── tools
│   └── wasm2go
│       └── main.go
│           └── Transpiler
│               ├── TranspileModule(wasmBytes) -> hyrt_gen.go
│               └── EmitFuncTable(elementSection) -> var funcTable [N]func(...) uint64
├── runtime
│   └── go
│       └── hybridge
│           └── hyrt_gen.go
│               ├── var mem [wasmMemorySize]byte (Global linear memory array)
│               ├── var funcTable [N]func(...) uint64 (Read-only initialized in init())
│               └── HyNop(x uint64) uint64 -> direct Go execution (~5–10 ns/op)
```

---

## Architectural Details

1. **Build-Time Transpilation**:
   `tools/wasm2go/main.go` reads `hyrt.wasm` and generates `runtime/go/hybridge/hyrt_gen.go` containing native Go functions for every exported Wasm function.

2. **Linear Memory Array**:
   Linear memory is declared as `var mem [wasmMemorySize]byte`. `OwnHandle` arena slices interact directly via sub-slices (`mem[offset : offset+size]`) with zero allocations and zero GC pressure.

3. **Function Table Dispatch**:
   `var funcTable [N]func(...) uint64` is declared at package level and initialized once inside `init()` from active Element segment 0. All `call_indirect` opcodes dispatch directly via `funcTable[tableIdx](args...)` without lock contention or runtime reflection.
