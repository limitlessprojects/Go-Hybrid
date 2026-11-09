# [wildcard] Direct Go SSA IR lowering via hy-ssa: eliminating source code string emission, frontend AST loss, and parsing overhead

### The idea, in a paragraph
Hybrid currently emits plain Go source text through `hy-emit`, writing `.go` files to disk before calling `go build`. This intermediate text layer causes most of the compiler's Row 13 syntax issues, such as reserved keyword collisions, unread variable errors, and indentation bugs. It also drops the single-ownership proofs computed by `hy-check`, which forces Go's escape analysis to move values to the heap. We propose `hy-ssa` to lower checked AST nodes directly into Go SSA basic blocks. By attaching stack allocation certificates (`OpLocalAddr [STACK_CERTIFIED]`) to proven unique values, the compiler can guarantee stack allocation directly without Go garbage collection overhead.

### Which assumption it breaks
1. SPEC section 1.3 assumes Hybrid must compile to standard Go source text. Lowering directly to SSA IR treats the Go toolchain as a compiler backend rather than a text target.
2. It changes GUARANTEES.md Row 17. The current position relies entirely on Go's GC for heap memory safety. With `hy-ssa`, values proven uniquely owned at check sites receive fixed stack frame slots, bypassing Go's escape analysis.
3. It eliminates text-level syntax bugs. Emitting structured basic blocks removes keyword naming collisions and unused variable errors before disk writes.

### Why this is a breakthrough, not an increment
This addresses Active Solicitation Prompt 3 ("Attack the emit target"). Patches to `hy-emit` and `buildable.rs` adjust string templates, shadow tables, or synthetic blank reads. Those fixes cannot solve the underlying problem because converting structured compiler proofs into text strings drops semantic information.

Direct SSA lowering preserves compile-time ownership information through code generation. This gives Hybrid programs predictable stack allocation and removes the entire text formatting layer.

### How we would know it works (or doesn't)
We built a runnable spike in `spikes/w-direct-ssa-emit/`:
1. It lowers arithmetic, basic data types, diamond branching with phi nodes, loops with back-edges, and channel operations into SSA blocks (`spikes/w-direct-ssa-emit/ssa_lowering.py`).
2. It verifies that owned values receive stack certificates, while values whose addresses escape lose their certificate and register as heap allocations.
3. It confirms that Go keywords like `interface`, `default`, and `select` produce zero naming conflicts in SSA form.
4. All 25 unit tests pass locally (`python -m unittest discover -s spikes/w-direct-ssa-emit/tests -v`).

Milestone 2 plan:
- Gate 1 (Toolchain Stability): Test lowering against multiple Go compiler versions via an independent SSA IR adapter layer to prevent version breakage.
- Gate 2 (Memory Soundness): Verify stack frame pointer bitmaps (`GCStackMap`) and write barrier generation (`OpStoreWB`) to ensure runtime GC safety.
- Gate 3 (Allocation Threshold): Benchmark heap allocation reduction and compile throughput vs standard and pragma-annotated text generation.

### Cost honesty
This approach deprecates `compiler/crates/hy-emit` and replaces text templates with an SSA bridge. During the transition, `hy-ssa` can run behind a compiler flag (`hyb build --target=go-ssa`) so both paths can be compared before removing text emission.

### Track terms
- [x] I understand no prototype is required and failure-with-knowledge is a valid, paid outcome
- [x] I understand all outcomes (including declines and failures) are published in MOONSHOT.md

Author: @tmclaos
Settlement Coordinates: 0xAF9192689F02C02A513983Def82113290360Bb6B (USDC on Base)
