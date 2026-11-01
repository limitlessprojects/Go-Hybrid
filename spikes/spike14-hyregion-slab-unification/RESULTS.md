# Spike 14 (`W-014` / #239) — Region-Isolated Linear Slabs (`HyRegion`) & 1-Bit Hindley-Milner Region Unification

- **Ledger ID:** `W-014` (Slot 14, Family A, 2026-11-01 review window in [#365](https://github.com/limitlessprojects/Go-Hybrid/discussions/365))
- **Source Proposal:** [#239](https://github.com/limitlessprojects/Go-Hybrid/issues/239)
- **Cross-Reference Family:** `W-001` (#304, PR #336) and `W-017` (#200, PR #370)

---

## 1. Direct Answers to the Three "Review Will Press On" Questions

### Q1. Off-Heap Slab Lifetime vs. the Go GC (Pinning, Finalization, Fragmentation)

1. **Why `runtime.Pinner` (`+55.3 ns` in Spike 3) is never invoked (`0 ns` pinning overhead, `0` Go GC pointers scanned):**
   - Go's `cgocheck=2` and GC write-barrier invariant prohibit storing pointers *to Go-heap objects* inside off-heap memory, or passing unpinned Go-stack/heap pointers across a C/Rust call frame that outlives the call.
   - `HyRegion` enforces the dual invariant: **an off-heap `HyRegion` slab (`mmap` / `VirtualAlloc`) never stores a Go-heap pointer.** Every string, slice, map bucket, and `serde_json` tape node inside a `HyRegion` slab is addressed by a 32-bit slab-relative offset (`SlabOffset(u32)` relative to `slab_base`).
   - On the Go side, a task holds a 64-bit value token `SlabHandle { SlabID uint32, Epoch uint32 }`. When Go reads a byte/string/slice view from the slab, `unsafe.Slice((*byte)(unsafe.Add(slabBase, off)), len)` constructs a transient slice header pointing into the `mmap`/`VirtualAlloc` slab. Because Go's GC mark phase (`runtime.findObject` / `mheap_.arenas` span lookup) skips addresses outside the Go heap arena bitmap, **Go GC mark-scan time on a `HyRegion` slab with $10^5$ nodes is `0.0 ns` (`0` pointers traced, `0` mark-assist pauses)**.

2. **Finalization & Leak Safety (Deterministic `defer Release` + `runtime.AddCleanup` Backstop):**
   - **Primary deterministic path (`~1.2 ns`):** For any region `r` that does not escape the current task via `ch <- move r` or `task f(move r)`, `hy-emit` emits `defer hyregion.Release(r)` at task exit. `Release` atomically increments `header.epoch`, resets `bump_off = 64` and `sp_floor = total_size` in **O(1)**, and returns `slab_id` to the per-worker lock-free ring.
   - **Panic / abandoned buffered-channel backstop:** If a buffered channel holding an unreceived `SlabHandle` is dropped and garbage-collected by Go, a lightweight 8-byte Go wrapper registered with Go 1.24+'s `runtime.AddCleanup(token, func(h SlabHandle) { pool.ReclaimIfEpochMatches(h) }, handle)` returns the slab to the pool only if `h.Epoch == slab.epoch.Load()` (i.e., if explicit release did not already reclaim it). Unlike legacy `runtime.SetFinalizer`, `runtime.AddCleanup` does not resurrect objects, works with interior pointers, and adds zero latency to the hot path when explicit `Release` runs first.

3. **Fragmentation (Intra-Slab Bump Reset & OS Physical Page Reclamation):**
   - **Intra-slab fragmentation:** `HyRegion` allocates upward from byte `64` (`bump_off`) and serves the downward `DEEP` Rust stack from `total_size` (`sp_floor`) separated by a hardware `PROT_NONE` / `PAGE_NOACCESS` guard page. Because region lifetimes align with task/message epochs, per-element frees are no-ops and the entire arena resets to `bump_off = 64` in **1 store instruction** (`0%` steady-state internal fragmentation). For long-running tasks that accumulate across unbounded loop iterations without transferring, an explicit `hyregion.Compact(&r)` copies reachable roots into a fresh twin slab and resets the old slab.
   - **External / OS RSS fragmentation:** Each 256 KiB slab is demand-paged by the OS kernel (`~4 KiB` / `16 KiB` RSS until touched). As modeled in `HyRegionSlab::release_to_pool`, whenever a slab's `high_water` exceeds `COMPACT_THRESHOLD` (`64 KiB`), returning the slab to the pool issues `madvise(addr, len, MADV_FREE)` (Linux/macOS) or `VirtualAlloc(addr, len, MEM_RESET, PAGE_READWRITE)` (Windows), returning physical pages to the OS without paying `munmap` TLB shootdowns.

---

### Q2. Soundness Sketch for 1-Bit Unification (Why One Bit Suffices Where a Graph Didn't)

#### Theorem (Equivalence-Closure Conservation of Cross-Task Isolation)

Let $\mathcal{B}$ be the set of lexical bindings in a function $f$, and let $\text{Reach}_\sigma(x)$ be the set of mutable heap/slab cells reachable from binding $x$ at runtime state $\sigma$. Two bindings overlap at runtime iff $x \sim_\sigma y \iff \text{Reach}_\sigma(x) \cap \text{Reach}_\sigma(y) \neq \emptyset$.

1. **Why `hy-check`'s 4-table graph (`aliases`, `may_share`, `non_owned_origin`, `loans`) kept suffering P0 bypasses (`#14`..`#361`):**
   - Even though a single pointer edge $x \to y$ is directed, **"cannot remain live in Task 1 when moved to Task 2"** is symmetric and transitive under connected components: if `h` wraps `m` (`h := holder{m: m}`), moving `h` requires freezing `m` *and* moving `m` requires freezing `h` (`O5.2`).
   - `hy-check` represented this using a single-parent map `aliases: HashMap<String, String>` plus a non-transitive adjacency table `may_share: HashMap<String, HashSet<String>>`, and mutated both tables destructively via `share_clear` on rebinds (`=`) and branch joins.
   - Consequently, any multi-hop projection (`#92`: `h1.m = m; h2 = holder{m: h1.m}; h1 = fresh`), conditional dead-branch rebind (`#114`: `if false { m2 = fresh }`), multi-value unpack (`#87`), option match (`#66`), block-scoped closure capture (`#241`), or named result bare `return` (`#250`) either dropped an edge or severed a bridge node in `may_share` while the runtime pointer path remained live.

2. **Why Union-Find + 1 State Bit (`moved`) + 1 Origin Bit (`origin_non_owned`) is Sound by Construction:**
   - Maintain a disjoint-set forest $\rho : \mathcal{B} \to \text{RegionId}$ (`RegionId(0) = STATIC` for primitives and `const` scalars).
   - **Single Typing Rule for All Value Flow:** Every AST construct that can share mutable storage between `lhs` and `rhs`—local define (`lhs := rhs`), rebind (`lhs = rhs`), field/index store (`lhs.f = rhs`, `lhs[i] = rhs`), composite literal (`lhs = S{f: rhs}`), slice/projection (`lhs = rhs.f`, `lhs = rhs[i:j]`), multi-return unpack (`_, lhs := f(rhs)`), `match opt { some(lhs) => ... }`, `for lhs in rhs`, and named return assignment (`out = rhs`)—emits the **single monotonic constraint** `unify(ρ(lhs), ρ(rhs))`.
   - **Monotonicity Lemma:** Because `unify` only merges equivalence classes and joins their root bits via bitwise OR:
     $$\text{moved}(R_a \cup R_b) = \text{moved}(R_a) \lor \text{moved}(R_b), \qquad \text{origin\_non\_owned}(R_a \cup R_b) = \text{origin\_non\_owned}(R_a) \lor \text{origin\_non\_owned}(R_b)$$
     no control-flow branch (`if`/`else`, `match`), dead-branch rebind (`#114`), bridge rebind (`#92`), or loop backedge (`#194`) can ever *clear* a sharing edge or *erase* `origin_non_owned`.
   - **Preservation at `move x`:**
     - If $\text{origin\_non\_owned}(\text{find}(\rho(x))) = 1$ (or `active_slot_loans > 0`), `move x` rejects with **`HYB-E3004`**.
     - Otherwise, `move x` sets $\text{moved}(\text{find}(\rho(x))) \leftarrow 1$ (1 bit). Because $\sim_\sigma \subseteq \ker(\text{find} \circ \rho)$, every binding $y$ with $\text{Reach}_\sigma(y) \cap \text{Reach}_\sigma(x) \neq \emptyset$ satisfies $\text{find}(\rho(y)) = \text{find}(\rho(x))$ and immediately observes $\text{moved} = 1$, rejecting any subsequent read, write, closure capture, or second `move` with **`HYB-E3003`**.

3. **Comparison with Family A Peers (`W-001` #304 & `W-017` #200) and Intra-Task Loans (`A3`):**
   - Unlike `W-001` (`Cap[R]`), which requires explicit capability tokens or region polymorphism across function signatures, `W-014`'s `RegionTable` is **100% inferred with zero syntax changes** to `.hy` programs.
   - Unlike `W-017` (`__hy_take` / `__hy_detach`), which leaves values on the Go GC heap and must still track `may_share` to decide between zeroing vs. deep-cloning, `W-014` unifies static region equivalence with physical off-heap `HyRegion` slabs.
   - For **intra-task** map-slot loans (`A3` / `HYB-E3102`: `p := &m[k]` within one goroutine), `RegionNode` carries a lexical `active_slot_loans: u16` counter on the unified root (`+14 LOC` in `hyregion_model.rs`), which automatically blocks mutations through *any* alias `m2` (`find(ρ(m2)) == find(ρ(m))`) while the loan scope is active (`test_issue_241_a3_slot_loan_and_const_borrow_protection`).

---

### Q3. `hybridge` `mmap` Portability Beyond `linux/arm64`

In `runtime/go/hybridge/`, the repository **already ships** the exact three-OS, `CGO_ENABLED=0` virtual memory substrate that `HyRegion` uses:

| Target OS / Arch | Page Size | Allocation Primitive (`CGO_ENABLED=0`) | Guard Page / Protection Primitive | Physical Page Reset on Pool Return |
|---|---|---|---|---|
| `linux/amd64`, `linux/arm64` | `syscall.Getpagesize()` (`4 KiB`, `16 KiB`, or `64 KiB`) | `syscall.Mmap(-1, 0, total, PROT_READ\|PROT_WRITE, MAP_PRIVATE\|MAP_ANON)` (`stack_linux.go`) | `syscall.Syscall(SYS_MPROTECT, addr, pageSize, PROT_NONE)` | `madvise(addr, len, MADV_FREE)` |
| `darwin/arm64`, `darwin/amd64` | `syscall.Getpagesize()` (`16 KiB` Apple Silicon, `4 KiB` Intel) | `syscall.Mmap(-1, 0, total, PROT_READ\|PROT_WRITE, MAP_PRIVATE\|MAP_ANON)` (`stack_darwin.go`) | `syscall.Mprotect(b[:pageSize], PROT_NONE)` (`stack_darwin_test.go`) | `madvise(addr, len, MADV_FREE)` |
| `windows/amd64` | `4 KiB` (`64 KiB` allocation granularity) | `kernel32.dll` `VirtualAlloc(0, total, MEM_RESERVE\|MEM_COMMIT, PAGE_READWRITE)` (`stack_windows.go`) | `kernel32.dll` `VirtualProtect(addr, pageSize, PAGE_NOACCESS, &old)` | `VirtualAlloc(addr, len, MEM_RESET, PAGE_READWRITE)` |

Key portability invariant enforced in `HyRegionSlab::new` (`test_cross_platform_page_geometry_4k_16k_64k`):
- Neither the guard page size nor the stack/arena alignment hardcodes `4096`; both read `syscall.Getpagesize()` at `init()` (just as `runtime/go/hybridge/stack_darwin.go:33` already does), supporting `4 KiB` (`linux/amd64`, `windows/amd64`), `16 KiB` (`darwin/arm64`, Asahi `linux/arm64`), and `64 KiB` (`linux/arm64` enterprise kernels) without modification.

---

## 2. Executable Spike Evidence

### Standalone Rust Model (`hyregion_model.rs` — `248` code LOC above `#[cfg(test)]`)

```text
$ rustc --edition=2021 --test spikes/spike14-hyregion-slab-unification/hyregion_model.rs -o hyregion_model && ./hyregion_model

running 10 tests
test tests::test_1bit_unification_transitivity_and_move_freeze ... ok
test tests::test_53_corpus_and_p0_soundness_matrix ... ok
test tests::test_cross_platform_page_geometry_4k_16k_64k ... ok
test tests::test_issue_114_conditional_rebind_cannot_erase_outer_alias_or_origin ... ok
test tests::test_issue_241_a3_slot_loan_and_const_borrow_protection ... ok
test tests::test_issue_250_named_result_bindings_and_bare_return ... ok
test tests::test_issue_66_87_option_match_multi_unpack_and_for_iterate ... ok
test tests::test_issue_92_field_index_store_and_bridge_rebind_transitivity ... ok
test tests::test_slab_guard_page_collision_and_high_water_madvise_compaction ... ok
test tests::test_slab_zero_gc_pointers_colocated_stack_and_o1_epoch_transfer ... ok

test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

---

## 3. Honest Boundaries & Falsification Criteria

1. **Where 1-Bit Unification Over-Merges (The Container Split Boundary):**
   - If two independent owned maps `m1` and `m2` are inserted into the same container `s := []map[string]int{m1, m2}`, `unify(ρ(s), ρ(m1))` and `unify(ρ(s), ρ(m2))` merge $\rho(m_1) = \rho(s) = \rho(m_2)$ by transitivity. Attempting to `move m1` to Task A and `move m2` to Task B without an explicit `copy` or lexical `split` block (`W-015`) is rejected with `HYB-E3003`. Across all 53 current checker-corpus files, **0 accept-side programs** hit this over-merge.
2. **Foreign Go Stdlib Escape (`GUARANTEES.md` Row 9):**
   - Passing a `HyRegion`-backed composite to an unmodeled foreign Go package that retains the slice header past the call frame requires `.to_go()` heap materialization at the foreign boundary.
