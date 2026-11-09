# W-SSA feasibility spike: direct Go SSA IR lowering

- Proposal: Wildcard Track Prompt 3 solicitation (Attack the emit target)
- Author: @tmclaos
- Target review window: 2026-12-01
- Settlement rail: 0xAF9192689F02C02A513983Def82113290360Bb6B (USDC on Base)

## 1. Scope and questions evaluated

This spike tests three items for replacing plain Go text emission:
1. Direct SSA representation: Lowers checked AST nodes into Go SSA basic blocks (Block, Value, Op, Phi), avoiding keyword collisions and unused variable errors by design.
2. Stack allocation certificates: Directs proven single-owner values into fixed stack slots, bypassing Go escape analysis to prevent heap allocations.
3. Falsification check: Asserts that if an address escapes, the stack certificate is revoked and the value is treated as heap-allocated.

## 2. Reproduction

```bash
python -m unittest discover -s spikes/w-direct-ssa-emit/tests -v
```
