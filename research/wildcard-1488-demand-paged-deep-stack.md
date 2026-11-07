# Wildcard #1488 — Keep the sync ABI, make the deep stack demand-paged: a falsification of the stackless-Future route

**Issue:** [#1488](https://github.com/limitlessprojects/Go-Hybrid/issues/1488) · **Status:** design-only; no runtime changes in this PR.

## 0. Verdict up front

The proposal's stated mechanism — compile `DEEP`/`blocking` Rust boundary functions as stackless `Future` state machines whose `poll()` runs in `<512 B` on the caller's goroutine stack — cannot hold at a C-ABI boundary. The boundary function pointer (`callDeep1..4` in `runtime/go/hybridge/bridge.go:42,45,48,157`) is an opaque `unsafe.Pointer` to arbitrary Rust code, including third-party synchronous code with recursive frames (`serde_json` depth-127 is the issue's own example). You cannot make an opaque sync callee stackless; you can only rewrite every callee, including code we do not own.

What survives is less glamorous and much cheaper: **the 1 MiB is virtual, not resident.** Keep the synchronous ABI, replace the pooled fully-committed `mmap` stacks with reserved-but-uncommitted mappings (`PROT_NONE` reservation + `mprotect`/`mmap` growth on fault, or `MAP_NORESERVE`), and grow on demand. The headline hazard — "1 MiB resident per active deep call" — collapses to "touched pages per active deep call", with no scheduler rewrite.

## 1. Why the stackless route fails at this boundary

Three independent blockers, in order of hardness:

**1.1 The callee is opaque.** `callDeep1(fn unsafe.Pointer, a1 uintptr, stack uintptr)` switches `SP` onto `deepTops[i]` and calls `fn`. The caller never sees the callee's signature, generics, or `.await` points. A `Future`-based design requires the *callee* to be compiled as a state machine and the *caller* to drive it; neither side exists in the current ABI. Making it true means regenerating every boundary function from Rust source with an async transform — which breaks `serde_json`, `tokio`-free sync libraries, and every hand-written `extern "C"` shim.

**1.2 `<512 B` poll headroom is falsified by the very workload it targets.** The issue cites `serde_json` depth-127 as the reason the 1 MiB stack exists. A stackless parser still needs a frame per recursion level (the state machine holds the *data*, not the call frames' temporaries), and `core::future::Future` polling of a recursive descent parser is not flat. Pre-registering the measurement is the honest move: instrument `poll()` stack watermarks on the corpus's deepest parse and report the high-water mark. If it exceeds 512 B — which the existing depth-127 parse suggests — the `<512 B` claim is dead on arrival.

**1.3 Parking a Go `g` on `Pending` crosses two schedulers.** `Pending` only helps if something re-polls later. That requires the Rust future to register a waker that re-enters the Go scheduler, i.e. a full runtime bridge with its own bookkeeping — which is exactly the "two permanent architectural walls" the issue proposes to delete. The two-tier `entersyscallblock` pool exists because Go's scheduler already has a story for blocking syscalls; a custom future bridge would need to reproduce it, not avoid it.

## 2. What the numbers actually say

From `research/spike-report.md` and the bridge: crossing overhead is **2.82 ns sandbox / 4.36 ns worker** (row 1), and the deep path costs **19.19 ns sandbox / 42.41 ns worker** plus `1 MiB` of *virtual* address space per pooled stack. The 42 ns figure is dominated by the `SP` switch and pool lookup, not by page faults, because the pool pre-commits. Two consequences:

- The performance argument for stackless is weak: deep calls are already 6–15× the fast path, and the fast path (`Cross1S`, 3.37 ns) is untouched either way.
- The *memory* argument is the real one, and it is an accounting error: 1 MiB of `MAP_NORESERVE` address space is not 1 MiB of RSS. Measure `VmRSS` delta per active deep call before claiming the pool costs 1 MiB resident; the pool's actual resident cost is the touched pages of each active stack.

## 3. The salvageable proposal

Keep `callDeep1..4` synchronous. Change only how the stack is backed:

1. Reserve a large region per pool slot (`PROT_NONE`), commit on first touch via a SIGSEGV handler or `mprotect` in a guard page, and return pages to the pool on unwind.
2. Track high-water marks per slot and size the pool from measured maxima instead of a fixed 1 MiB.
3. Keep the guard page; the safety property (no silent overflow) is unchanged.

This deletes neither pool nor trampoline, and it does not touch `entersyscallblock`. It removes the resident-memory claim, which is the part that actually hurts at 717 threads.

## 4. Pre-registered tests (before any code)

1. **RSS honesty.** Measure `VmRSS` delta for 1, 10, 100 concurrent deep calls, pool version vs. demand-paged version. Prediction: demand-paged uses **≤ 25%** of pool RSS at 100 concurrent calls. If the pool is already near that, the whole premise is wrong and the issue should be declined with this number.
2. **Overhead bound.** Deep-call cost must stay within **+10%** of 42.41 ns worker / 19.19 ns sandbox. A demand-paging fault per call would blow this; a fault per *first touch* would not.
3. **Guard-page preservation.** A deliberately overflowing recursive callee must still fault at the guard page, not corrupt the neighbouring slot.
4. **Stackless counter-evidence.** Instrument `poll()` watermarks on the depth-127 `serde_json` parse. Report the number; if > 512 B, the `<512 B` premise is falsified with data.
5. **Thread-budget interaction.** 717-thread crash reproduction must be unchanged by the memory change (the two-tier blocking pool is orthogonal and stays).

## 5. What does not survive

- **The `<512 B` poll headroom.** Almost certainly false for the workloads that motivate the pool; test 4 settles it.
- **Deleting the two-tier blocking pool.** `entersyscallblock` is a Go-scheduler mechanism, not a stack mechanism; a stackless future does not remove the need to detach an `M` for a blocking syscall.
- **"Kill both pools in one change."** The memory fix and the blocking fix are independent; bundling them is how a good memory change gets rejected for an unrelated scheduler rewrite.

**What survives:** reserve-don't-commit. One ABI-preserving change, measurable in RSS, falsifiable in a day.
