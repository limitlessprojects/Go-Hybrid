# W-SSA feasibility spike: direct Go SSA IR lowering

- Proposal: Wildcard Track Prompt 3 solicitation (Attack the emit target)
- Author: @tmclaos
- Target review window: 2026-12-01
- Settlement rail: 0xAF9192689F02C02A513983Def82113290360Bb6B (USDC on Base)

## 1. Scope and questions evaluated

This spike evaluates four core areas for replacing plain Go text emission:
1. Direct SSA representation: Lowers checked AST nodes into Go SSA basic blocks (Block, Value, Op, Phi), avoiding keyword collisions, unread variable errors, and syntax bugs.
2. Stack allocation certificates: Allocates proven single-owner values directly into local stack slots (`OpLocalAddr [STACK_CERTIFIED]`), eliminating Go GC heap escapes.
3. GC pointer bitmap and write barrier safety: Generates stack frame pointer bitmasks (`GCStackMap`) and inserts write barriers (`OpStoreWB`) during active GC mark phases.
4. Falsification checks: Validates that escaping addresses immediately revoke stack certification and fall back to heap allocation.

## 2. Reproduction

Run unit test battery (25 tests):
```bash
python -m unittest discover -s spikes/w-direct-ssa-emit/tests -v
```

Run comparative lowering benchmark:
```bash
python spikes/w-direct-ssa-emit/benchmarks/benchmark_ssa.py
```
