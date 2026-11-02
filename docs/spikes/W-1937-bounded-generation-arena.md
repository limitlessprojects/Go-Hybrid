# Spike `W-1937`: Workload-Measured Bounded Borrow/Release Arena (`#1937`)

Establishes size-histogram baselines (`alloc.hy` / `load.hy`) and implements an explicit bounded borrow/release arena with per-P shard fast paths, fail-closed slot cap, and generation-counter handle validation (catching double-release and stale-handle access).

## Artifacts
- `spikes/w1937-bounded-generation-arena/bounded_arena.py`: Histogram analyzer and generation-checked per-P shard arena.
- `spikes/w1937-bounded-generation-arena/tests/test_w1937_bounded_arena.py`: 5 deterministic unit tests.
