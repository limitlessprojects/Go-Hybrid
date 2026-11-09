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
    └── Stack Allocation Certificates (OpLocalAddr [STACK_CERTIFIED])
```

## 3. Opcodes and semantics

| Opcode | Arguments | Meaning |
|---|---|---|
| OpLocalAddr | aux_str: var_name | Stack frame slot for single-owner places |
| OpStore | [slot, val] | Memory write into local stack frame slot |
| OpLoad | [slot] | Memory read from local stack frame slot |
| OpPhi | [val1, val2] | SSA merge across branching edges |
| OpChanSend | [chan, val] | Direct channel send |
| OpChanRecv | [chan] | Direct channel receive |
| OpBranchCond | [cond] | Conditional branch to then/else blocks |
| OpReturn | [val] | Function return |
