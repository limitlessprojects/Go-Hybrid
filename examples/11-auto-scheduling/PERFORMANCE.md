# Auto Scheduling Performance Analysis

## Overhead vs regular Hybrid

### What the auto scheduler adds

1. **Compile-time cost**:
   - Effect analysis: O(n) per task block, where n = statements in the block
   - Conflict detection: O(t²) where t = number of tasks (pairwise checks)
   - Greedy layering: O(t²) worst case, O(t * layers) average
   - **Total**: negligible for typical programs (< 10 tasks), microseconds

2. **Runtime cost (per auto block)**:
   - `sync.WaitGroup` allocation: ~20ns per layer
   - `wg.Add(1)` + goroutine spawn: ~100-200ns per task (Go goroutine startup)
   - `wg.Wait()`: synchronization overhead, minimal if tasks do real work
   - **Single-task layers**: no goroutine overhead (runs directly inline)

### Expected overhead

| Scenario | Overhead vs sequential | Overhead vs manual goroutines |
|---|---|---|
| 1 task, 1 layer | ~0% (runs inline) | +100% (but no concurrency needed) |
| 2 independent tasks, 1 layer | ~100-200ns (goroutine spawn) | ~0% (same as manual) |
| 4 tasks, 2 layers (2 parallel each) | ~200-400ns total | ~0% (same as manual) |
| 10 tasks, 5 layers | ~1-2μs total | ~0% (same as manual) |

### Key observations

- **No runtime scheduler in our code**: the scheduling decision is made at compile time.
  The emitted Go code uses plain goroutines + WaitGroup, which is exactly what a
  developer would write by hand.

- **No lock contention**: the layering is static, so there's no runtime dependency
  tracking or lock acquisition overhead.

- **Single-task layers are free**: if a layer has only one task, it runs inline
  (no goroutine spawn, no WaitGroup). This avoids unnecessary overhead for
  sequential parts of the workload.

### Where overhead could be excessive

1. **Very small tasks**: if each task does only a few nanoseconds of work,
   goroutine spawn overhead dominates. But in that case, the tasks are too small
   to benefit from parallelism anyway.

2. **Deep dependency chains**: if every task conflicts with every other task,
   we get t layers of 1 task each — fully sequential, but with the overhead
   of effect analysis. However, this is still safe (no data race) and the
   overhead is compile-time only.

### Conclusion

The overhead is **negligible for real-world workloads**:
- Compile-time: microseconds, unmeasurable in practice
- Runtime: identical to hand-written goroutine + WaitGroup code

The tradeoff is worth it: you get safe concurrency without manual ownership
annotations, and the compiler automatically extracts parallelism where it's safe.
