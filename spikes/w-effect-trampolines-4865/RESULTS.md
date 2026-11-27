# W-4865 Spike Results — Algebraic Effect Trampolines Across Go↔Rust

**Proposal:** [#4865](https://github.com/limitlessprojects/Go-Hybrid/issues/4865) | **Author:** `@Urius-beta` | **Scope:** isolated to `spikes/w-effect-trampolines-4865/` (no edits to `MOONSHOT.md`, `GUARANTEES.md`, or `.github/workflows/`)

**Reproduce:**
```bash
python3 spikes/w-effect-trampolines-4865/effect_trampoline_verifier.py --check
python3 -m unittest discover -s spikes/w-effect-trampolines-4865/tests -p "test_*.py" -v
```

---

## 1. Pre-Registered Gates & Falsifiers (`P1`–`P3`, `F1`–`F5`)

| ID | Gate / Falsifier | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **P1** | Static `@[effect_session(...)]` admission (`HYB-E3801`–`HYB-E3805`) on 3 minimal `.hy` fixtures (`hy/01`, `hy/02`, `hy/03`) | `01-streaming-effect-session.hy`: `0` diags; `02-structured-result-error.hy`: `0` diags; `03-rejected-effect-violations.hy`: all 5 rules triggered (`HYB-E3801`..`HYB-E3805`) | **PASS** |
| **P2** | Non-reentrant ping-pong state machine: interleaved `ReadChunk` + `LogMetric` streaming (`N=8`) and structured `Result<T, E>` error return via terminal `ERR` tag | Streaming: `sum=72`, `metrics=8`, `pins_acquired=1`, `max_rust_depth=1`, `owned_resources=0`, `state=DONE`; Error path: `DecodeErr{code=422, offset=23}`, `pins_acquired=1`, `owned_resources=0`, `state=ERR_TERMINATED` | **PASS** |
| **P3** | ABI cost model using `interfaces/abi-types.md` measured constants (`fast=2.42 ns`, `pin=55.0 ns`, `yield=2.50 ns`) + sensitivity sweep (`yield=10 ns`, `20 ns`) | Break-even at `N=2` (`1.77x`); `N=8` (`4.87x`); `N=32` (`8.65x`); `N=1000` (`11.54x`). Under 8× stress (`yield=20 ns`), `N=2` stays positive (`1.15x`) and `N=1000` reaches `2.55x` | **PASS** |
| **F1** | **Falsifier — Go handler panics mid-stream**: without emitted `defer frame.abort(gen)`, Rust RAII state leaks across Go unwind; remedy binds `defer frame.abort(gen)` + `defer pinner.Unpin()` before loop entry | Unprotected: `state=YIELDED`, `owned_resources=1` (**leaked as predicted**). Remedy: `state=ABORTED`, `owned_resources=0`, `pins_released=1` | **FIRED + REMEDY PASS** |
| **F2** | Active-frame re-entrancy (`resume` called while `state == RUNNING`) | Rejected fail-closed (`HYB-RT-REENTRANT`) | **FIRED** |
| **F3** | Stale generation token replay (`gen != frame.gen`) | Rejected fail-closed (`HYB-RT-GEN`) | **FIRED** |
| **F4** | Double-resume after terminal `DONE` / `ERR_TERMINATED` / `ABORTED` | Rejected fail-closed on all 3 terminal states (`HYB-RT-CONSUMED`) | **FIRED** |
| **F5** | Scratch buffer length overflow (`payload_len > scratch_cap`) | Rejected fail-closed (`HYB-RT-BOUNDS`) + deterministic `ABORTED` cleanup (`owned_resources=0`) | **FIRED** |

---

## 2. ABI Cost Model & Sensitivity Sweep (`interfaces/abi-types.md` §1–§2 Constants)

Formula:
- Per-call pinning baseline: `T_per_call(N) = N * (2.42 + 55.0) ns`
- Single-pin effect session: `T_session(N) = 55.0 + N * (2.42 + t_yield) ns`

| Yields (`N`) | Per-Call Pin (`ns`) | Session (`t_yield=2.5 ns`) | Speedup (`2.5 ns`) | Speedup (`10 ns`) | Speedup (`20 ns`) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 57.42 | 59.92 | 0.96× | 0.85× | 0.74× |
| 2 | 114.84 | 64.84 | **1.77×** | **1.44×** | **1.15×** |
| 8 | 459.36 | 94.36 | **4.87×** | **2.98×** | **1.96×** |
| 32 | 1837.44 | 212.44 | **8.65×** | **4.06×** | **2.38×** |
| 1000 | 57420.00 | 4975.00 | **11.54×** | **4.60×** | **2.55×** |

Honest boundary: for `N = 1` single-shot crossings with no callbacks, plain direct calls remain faster (`0.96×`); `@[effect_session]` is opt-in and targets streaming / callback / loop-amortized crossings (`N >= 2`).

---

## 3. Proposed Before / After Spec Text (For Review — Not Mutated in This Spike)

### `GUARANTEES.md` — Row 19

**Before:**
> `| 19 | Memory safety, at the boundary | Owning-side handle tables with generation tags; copy-small/pin-large borrows | **Guaranteed** by the Spike-3 design + tests | §9.0 three-layer model; interfaces/abi-types.md |`

**After (proposed upon M2 landing):**
> `| 19 | Memory safety, at the boundary | Owning-side handle tables with generation tags; copy-small/pin-large borrows; non-reentrant `@[effect_session]` trampolines with single-pin `PinLease`, POD `Result<T,E>` terminal tags, and emitted `defer abort` teardown (`HYB-E3801`–`HYB-E3805`) | **Guaranteed** by Spike-3 + W-4865 effect-trampoline contract | §9.0 three-layer model; interfaces/abi-types.md §2; spikes/w-effect-trampolines-4865/RESULTS.md |`

### `interfaces/abi-types.md` — §2 Rows (`[]T`, `error`, `callbacks`)

**Before:**
- `error values`: *per Spike 5 (pending) — until then: crossing an error is **forbidden***
- `function pointers / callbacks`: ***forbidden in v0** (no re-entrancy across the boundary)*
- `§6 Open items`: *Pin-amortization protocol for loops (pin once, many calls) needs a spelled-out generator pattern + test.*

**After (proposed upon M2 landing):**
- `Result[T, E]` (POD `E`): terminal `ReturnTag::ERR` in the effect trampoline returns `E` by value with deterministic Rust RAII drop and zero cross-boundary unwinding.
- Non-reentrant callbacks (`@[effect_session(E1, ...)]`): Rust yields `ReturnTag::EFFECT_*` to a Go-side ping-pong loop that runs handlers on the normal splittable Go stack (`max_rust_depth == 1`, zero Rust←Go re-entrancy).
- Single-pin session amortization: Go trampoline pins the session scratch buffer once before loop entry and unconditionally binds `defer frame.abort(gen)` + `defer pinner.Unpin()`, eliminating per-call 55 ns pin overhead for `N >= 2`.
