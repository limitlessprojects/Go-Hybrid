# W-016 Spike: Eliminate Checker/Emitter Semantic Split via Explicit Desugared MIR (`hy-mir`)

**Track:** Wildcard Track ($500)
**Issue Reference:** #695
**Author:** hardik4love-lang (shruhicollections@gmail.com)

## Core Architectural Invariant
Every implicit Go emission transformation (auto-addressing value arguments at pointer parameters,
composite `const` to mutable `var` storage lowering, and map-slot loan writebacks) is lowered into an explicit
instruction in `hy-mir` BEFORE `hy-check` runs.

## Structure
```
AST (hy-syntax) -> Lowering -> hy-mir (explicit AddrOf/AllocMut/Store) -> hy-check -> hy-emit (1:1 pretty printer)
```

## Falsifiable Proof
By verifying invariants directly on explicit MIR nodes, `hy-emit` is relieved of shadow type analysis and
cannot introduce unproven pointer aliasing.
