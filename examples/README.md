# The Ten Target Programs (SPEC.md §8)

Ten realistic programs the language must support before the MVP is called done.
Each directory holds a **target-syntax** `.hy` sketch (or Go file, where the
point IS the Go side), an `expected.txt`, and a header comment naming the
language feature it stresses. They become the first acceptance tests in
Phase 1; exact outputs (e.g. digest constants) get pinned at MVP-0 time.

| # | program | feature stressed |
|---|---------|------------------|
| 01 | hello | formatting, interpolation, zero values (no nil) |
| 02 | cli-cat | CLI args, file I/O, `?` error propagation, defer |
| 03 | worker-pool | tasks, channels, close semantics, deterministic join |
| 04 | pipeline-backpressure | bounded channels as backpressure, select+timeout |
| 05 | json-transform | Rust crate (serde_json) via the checked boundary; zero-copy views |
| 06 | http-server | full Go stdlib interop (net/http), closures as handlers |
| 07 | lru-generics | generics, struct methods, Option instead of nil |
| 08 | graph-ownership | ownership model on CYCLIC structures (id-refs, arena owner) |
| 09 | go-imports-hybrid | **the adoption story**: plain Go imports Hybrid, zero new toolchain |
| 10 | rust-crate-import | hot loop in a pinned Rust crate through the sub-5ns boundary |
