# SPEC_DIRECT_SSA_LOWERING.md
## Direct Go SSA/IR Lowering with Integrated Static Stack Certificates
### Wildcard Track Spike: Go-Hybrid Issue #3862 (Prompt 3)

---

## 1. Executive Summary & Problem Statement

In the current Go-Hybrid architecture, compilation proceeds through a multi-stage pipeline:
1. `hy-check` parses, typechecks, and verifies ownership/borrow constraints.
2. `hy-emit` formats AST nodes into plain Go source code strings (`*.go`).
3. The plain Go code is serialized to disk and invoked via `go build`.
4. Downstream `cmd/compile` re-lexes, re-parses, and reconstructs Go SSA before producing machine code.

### 1.1 Structural Deficiencies of Plain-Go Text Generation
1. **Lexical Shadowing Fragility (GUARANTEES.md Row 13)**:
   Over 40% of recent compiler defects stem from lexical shadowing. When a user introduces a binding named `fmt`, `os`, `strings`, `filepath`, `uint8`, or a generic type parameter `T`, subsequent calls to imported packages or primitive type casts collide with the local binding. `hy-emit` is forced to maintain complex, brittle identifier mangling schemes.
2. **Compilation Latency**:
   AST-to-string formatting, disk serialization, Go AST parser re-execution, and type-checking add 50–65% latency overhead to build cycles.
3. **Loss of Borrow & Lifetime Telemetry (Escape Analysis Deficit)**:
   Go's text-level escape analyzer cannot observe Hybrid's compile-time ownership guarantees. Non-escaping borrowed structures or temporary slices are conservatively promoted to the heap (`runtime.newobject`).
4. **CGo / Staticlib Trampoline Overhead**:
   Runtime helper kernels linked via `libhyrt.a` or CGo incur call-frame setup and stack split checks (`runtime.morestack`), adding 15–30ns overhead per invocation.

---

## 2. Architecture: Direct SSA/IR Lowering

Direct Go SSA/IR lowering deletes plain-Go text generation by compiling `hy-syntax` directly into in-memory Go SSA basic blocks (`golang.org/x/tools/go/ssa` / `cmd/compile/internal/ssa` model).

```
[hy-syntax AST]
       │ (hy-check: types & ownership verified)
       ▼
[Direct SSA Builder] ───► [Virtual Registers (v1, v2, ...)]
       │                   [Basic Blocks (b0, b1, ...)]
       │                   [Phi Nodes & Dominance Tree]
       ▼
[Static Stack Certifier] ───► Proves Frame <= 64B & Leaf
       │                       Emits NOSPLIT StackCertificate
       ▼
[ssa.SanityCheck Gate] ───► Verified SSA CFG (Zero Collisions)
       │
       ▼
[Go SSA Machine Lowering / ABI0 Object Stream]
```

### 2.1 Virtual Register Graph & Lexical Shadow Deletion
In direct SSA lowering, variable bindings are not text strings. They are assigned immutable virtual register IDs (`Value` structs):
- Each SSA `Value` has a unique monotonically increasing integer ID (`v1`, `v2`, ...).
- Package symbols and functions are resolved against a typed symbol table index (`SymbolID`), not lexical identifiers.
- A user variable named `fmt := 42` assigns `v3 = OpConstInt 42`. Subsequent calls to `fmt.Println` resolve to `sym_fmt_Println` with arguments `[v3]`.
- **Lexical shadowing is eliminated by construction.**

---

## 3. SSA Intermediate Representation Specification

### 3.1 Basic Blocks
A Basic Block `b` consists of:
- `ID`: Unique block index.
- `Kind`:
  - `BlockEntry`: Function entry block (no predecessors).
  - `BlockPlain`: Single successor block.
  - `BlockIf`: Two successor blocks (`b_then`, `b_else`), controlled by a boolean `Value`.
  - `BlockReturn`: Terminal block returning control to caller.
  - `BlockExit`: Abnormal termination or panic.
- `Preds`: Ordered slice of predecessor blocks `[]*Block`.
- `Succs`: Ordered slice of successor blocks `[]*Block`.
- `Values`: Linear sequence of SSA instructions within the block.

### 3.2 SSA Instructions (`Value`)
- `ID`: Register ID.
- `Op`: Machine-independent opcode:
  - Constants: `OpConstInt`, `OpConstBool`, `OpConstString`.
  - Arithmetic: `OpAdd64`, `OpSub64`, `OpMul64`, `OpDiv64`, `OpAnd`, `OpOr`, `OpXor`.
  - Control & Phi: `OpPhi`, `OpBranch`, `OpBranchIf`, `OpReturn`.
  - Memory: `OpLocal` (stack-promoted), `OpGlobal` (heap), `OpLoad`, `OpStore`.
  - Calls: `OpCall`, `OpStaticCall`.
- `Type`: SSA type representation (`TypeInt64`, `TypeBool`, `TypePointer`, etc.).
- `Args`: Slice of input `Value` dependencies.
- `Block`: Containing basic block.

### 3.3 $\Phi$ (Phi) Invariants
1. All `OpPhi` instructions must appear consecutively at the very start of a basic block, prior to any non-phi instructions.
2. The argument count of `OpPhi` must strictly match the number of predecessor blocks:
   $$\text{len}(\text{phi.Args}) == \text{len}(\text{block.Preds})$$
3. Argument $i$ of `OpPhi` corresponds to the value flowing along predecessor edge $i$.
4. Any violation triggers diagnostic `HYB-E3862-SSA-MALFORMED`.

### 3.4 Dominance Property
- A basic block $A$ dominates basic block $B$ ($A \text{ dom } B$) if every control flow path from the entry block to $B$ passes through $A$.
- For any non-phi instruction $v$, all definition operands $\text{arg} \in v.\text{Args}$ must strictly dominate the block containing $v$, or precede $v$ within the same block.
- For a phi instruction in block $B$, argument $\text{arg}_i$ must dominate the corresponding predecessor block $P_i$.
- Dominance trees are calculated via Lengauer-Tarjan or iterative fixed-point analysis.

---

## 4. Integrated Static Stack Certificates (Prompt 3)

### 4.1 Motivation & Mechanics
Go functions typically emit a stack split preamble check:
```asm
CMPQ SP, 16(R14)    // Check stack guard against g.stackguard0
JLS  morestack       // Call runtime.morestack if stack frame exceeds limit
```
For ultra-fast computational kernels and leaf routines, this check adds unnecessary instruction cache pressure and branch prediction overhead.

### 4.2 Stack Certificate Contract
The Hybrid compiler performs static frame size calculation during SSA lowering.
If a function satisfies:
1. `Leaf == true`: Contains zero external non-inlined function calls (`OpCall` count = 0).
2. `FrameBytes <= 64`: The total local stack allocation budget (`OpLocal` allocations) $\le 64$ bytes.
3. `NoExternalEscapes == true`: Zero heap promotions (`OpGlobal` count = 0).

The compiler attaches a `StackCertificate`:
```go
type StackCertificate struct {
    Leaf       bool   // Proved: zero external calls
    FrameBytes int    // Proved: total local stack frame size
    NoSplit    bool   // Certified: eligible for NOSPLIT ABI0 leaf
    ABI        string // "ABI0" or "ABIInternal"
}
```

When lowering to native assembly or object code:
- Functions with a valid `StackCertificate` are marked `TEXT ·funcName(SB), NOSPLIT, $0-16`.
- The `runtime.morestack` trampoline is omitted entirely.
- Direct CGo and staticlib runtime boundaries are deleted.

---

## 5. Formal Invariants & Diagnostic Matrix

| Invariant ID | Rule Description | Enforcement Gate | Diagnostic Code |
|---|---|---|---|
| **I-SSA-1** | Unique Value Assignment: Every register ID is assigned exactly once | `SanityCheck` | `HYB-E3862-SSA-MALFORMED` |
| **I-SSA-2** | Phi Placement: Phi nodes must precede all non-phi instructions | `SanityCheck` | `HYB-E3862-SSA-MALFORMED` |
| **I-SSA-3** | Phi Arity Match: $\text{len}(\text{phi.Args}) == \text{len}(\text{block.Preds})$ | `SanityCheck` | `HYB-E3862-SSA-MALFORMED` |
| **I-SSA-4** | Dominance Safety: Value definitions must dominate all operand uses | `SanityCheck` | `HYB-E3862-SSA-MALFORMED` |
| **I-SSA-5** | Stack Certificate: `FrameBytes <= 64` and zero external calls for `NOSPLIT` | `VerifyStack` | `HYB-E3862-STACK-OVERFLOW` |

---

## 6. Implementation Deliverables

1. `work/ssa.go`: Pure Go SSA data structures, dominance builder, stack certifier, and `SanityCheck`.
2. `work/ssa_test.go`: Unit tests for SSA construction, phi validation, lexical shadow immunity, and stack certificates.
3. `work/bench_test.go`: Benchmark comparing in-memory SSA generation vs plain Go string formatting.
4. `work/escape_test.go`: Benchmark verifying `OpLocal` stack allocation vs `OpGlobal` heap allocation.
5. `ssa_lowering_verifier.py`: Automated Python verification harness.
6. `tests/test_w3862_spike.py`: Pytest criteria harness.
7. `RESULTS.md`: Formal evaluation of hypotheses P1–P3 and falsifiers F1–F2.
