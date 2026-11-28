# W-SSA design specification: direct Go SSA IR lowering

## 1. Overview
The current compiler pipeline uses a lossy round-trip:
hy-syntax AST -> hy-check (verifies ownership and loans) -> hy-emit (writes Go text) -> disk file -> go build (re-parses Go text, runs Go escape analysis).

This pipeline has two problems:
1. Row 13 syntax issues: Keyword collisions, indentation errors, and unread variable bindings in generated text.
2. Loss of ownership information: Go's escape analysis does not see Hybrid ownership proofs, moving non-trivial values onto the garbage collector heap.

## 2. SSA IR model
hy-ssa lowers checked AST nodes directly into Go SSA basic blocks:

```
[ Checked AST (hy-check) ]
           │
           ▼
[ hy-ssa Lowering Engine ]
    ├── SSA Value Generation (OpConst, OpAdd, OpSub, OpMul, OpDiv, OpEq)
    ├── Control Flow Graph Construction (Branches, Loop Headers, Back-Edges)
    ├── Phi Node Placement (SSA Merges)
    ├── Stack Allocation Certificates (OpLocalAddr [STACK_CERTIFIED])
    ├── GC Stack Map Generator (1 bit per pointer word)
    ├── Active Write Barrier Tracking (OpStoreWB)
    └── Toolchain Version Adapter (Go 1.21, 1.22, 1.26.9)
```

## 3. Opcodes and semantics

| Opcode | Arguments | Meaning |
|---|---|---|
| OpLocalAddr | aux_str: var_name | Stack frame slot for single-owner places |
| OpStore | [slot, val] | Memory write into local stack frame slot |
| OpStoreWB | [slot, val] | Write-barrier guarded memory write during active GC mark |
| OpLoad | [slot] | Memory read from local stack frame slot |
| OpPhi | [val1, val2, ...] | SSA merge across branching edges |
| OpChanSend | [chan, val] | Direct channel send |
| OpChanRecv | [chan] | Direct channel receive |
| OpSelect | aux_int: num_cases | Multi-way channel communication branch |
| OpBranchCond | [cond] | Conditional branch to then/else blocks |
| OpBranch | none | Unconditional jump |
| OpDefer | [call_val] | Registration of deferred call frame |
| OpBreak | aux_str: label | CFG loop exit branch |
| OpContinue | aux_str: label | CFG loop back-edge branch |
| OpReturn | [val] | Function return |

## 4. GC pointer bitmap and write barrier compliance
For stack-certified frames:
- Every allocated frame slot records `is_ptr` status.
- `GCStackMap` computes pointer bitmasks (`0b10...`) so runtime GC safepoints only trace valid heap references.
- Writes to pointer-holding stack slots under concurrent GC mark phases automatically lower to `OpStoreWB` to preserve GC work-buffer invariants.
