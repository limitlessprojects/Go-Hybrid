# Monotone task regions: finite transition-model experiment

Companion experiment for [proposal #482](https://github.com/limitlessprojects/Go-Hybrid/issues/482),
whose original proposal credit remains with @Arnavquartz. Baseline:
`f76da5e3d8d447342888de204368abb8df399647`.

## Preregistered boundary (before implementation or execution)

This experiment addresses the proposal's **first** evaluation step only. It
does not implement Hybrid syntax, effect inference, lowering, a synchronized
Go runtime, or a benchmark. It cannot establish a language safety guarantee.

Use a closed, finite universe of integer facts, one opaque set, and a fixed
list of finite task programs. The admitted instructions are:

- `put(S)`: atomically union a fixed finite set into the cell; return unit.
- `await(S)`: enabled exactly when nonempty fixed `S` is included in the
  cell; return exactly `S`, never the current cell.
- A sorted snapshot is available only after **all** task programs finish.

Task observations retain task identity and local program order. Global event
order is not an observation. Every enabled task step is one atomic transition;
enumerate every maximal interleaving, including maximal blocked executions.
The model has no fairness, cancellation, timeouts, dynamic task creation,
foreign calls, live iteration, mutable elements, or memory bound claim beyond
the explicitly finite input. Admission here validates model IR, not `.hy`.

## Predictions and falsifiers

1. Seeded diamond propagation: all maximal schedules complete with one final
   sorted snapshot and one vector of per-task threshold observations.
2. Unseeded cyclic waits: exploration reaches a blocked terminal state, with
   zero completed schedules and no permitted snapshot. Adding a seed should
   permit completion. Quiescence alone is not completion.
3. Over a bounded generated family, all completed schedules agree; report
   blocked schedules separately, never silently drop them.
4. Three deliberately **unadmitted** mutants must exhibit concrete pairs of
   schedules with different observations or snapshots: live-set read,
   added/not-added return from put, and overwrite instead of union.
5. Reject empty/nonfixed thresholds, out-of-universe or nonscalar facts, and
   unknown effects/operations in the model IR. Refuse premature snapshots.

Any admitted example with two completed answers falsifies this model's
schedule-independence prediction. Any accepted snapshot while a task remains
blocked falsifies its join boundary. Missing negative witnesses invalidate
the experiment rather than count as success. A passing finite experiment is
not an unbounded proof, compiler validation, or a performance result.

## Execution results

Preregistered in commit `58356461`, then implemented and run with Python
3.12 on Linux/aarch64. Reproduce from the repository root:

```sh
python3 -m unittest discover -s spikes/monotone-task-regions-482 -p 'test_*.py' -v
python3 spikes/monotone-task-regions-482/model.py
```

The 16 tests passed. Literal machine-readable experiment output is in
[`results.json`](results.json); each negative control includes two schedules
and their actual observations. Task numbers in a schedule identify which
task takes its next atomic instruction. Replaying them is tested.

| Case | Completed schedules | Blocked schedules | Distinct completed answers |
| --- | ---: | ---: | ---: |
| Seeded diamond (seed produced by a task) | 6 | 0 | 1 |
| Unseeded two-task cycle | 0 | 1 | 0 |
| Same cycle with initial fact `0` | 1 | 0 | 1 |
| Forbidden live-set read | 2 | 0 | 2 |
| Forbidden added/not-added return | 2 | 0 | 2 |
| Forbidden overwrite | 2 | 0 | 2 |

The diamond's final snapshot is `(0, 1, 2, 3)`. The two branch tasks observe
only `(0,)`; the downstream task observes only its `(1, 2)` threshold, even
though the cell also contains `0` when it runs. The unseeded cycle has no
enabled step: the empty schedule is one maximal **blocked** execution with
no completed answer. Neither it nor a region with
one finished sibling and one blocked sibling permits a snapshot.

### Generated family and independent oracle

Universe `{0, 1}` has three nonempty subsets. The instruction alphabet has
six operations: `put(S)` and `await(S)` for each such subset. For each task,
enumerate all instruction sequences of length 0, 1, or 2: `1 + 6 + 36 = 43`.
Try every ordered pair of tasks and every one of four initial sets:
**4 × 43 × 43 = 7,396 programs**.

Enumeration visits **24,057 maximal schedules**: **21,782 completed** and
**2,275 blocked**. Of the programs, **5,377 complete** and **2,019 block**.
No program has a mixture of completed and blocked maximal schedules.
Every terminal state agrees with a separate round-based closure interpreter:
zero discrepancies across all 24,057 comparisons. That comparison includes
facts, program counters, and the task-indexed observation vector, even for
blocked states; it does not simply compare final set sizes. The oracle
does not call the explorer's transition function.

An independent combinatorial test also pins all six interleavings of two
two-step writers. This guards against an explorer that samples just one
scheduler order and mistakenly calls the result exhaustive.

### What the negative controls establish

- **Live read:** schedules `(0, 1)` and `(1, 0)` both finish at `{0}`, but
  the reader observes `{0}` versus the empty set.
- **Put flag:** two tasks each insert `0`. They finish at `{0}`, but the
  returned flags by task identity are `(true, false)` versus `(false, true)`.
  Comparing a global bag of flags would miss this difference.
- **Overwrite:** writers of `0` and `1` finish at `{1}` versus `{0}`.

The negative modes deliberately bypass the proposed semantics. Normal
admission rejects the live read; the other two are implementation mutants
of an admitted `put`, not additional certified operations. A bad runtime
returning a flag or overwriting storage would fail these controls.

## Interpretation and remaining boundary

For this particularly small IR, a completed program's final cell is its
initial set union every `put` operand, and every threshold observation is
fixed by its instruction. Enabled steps in distinct tasks commute: union
cannot disable an enabled positive await, and awaiting does not change
the cell. Each step consumes one instruction, so there is no infinite
transition path. These facts explain the result; the bounded enumeration
is a reproducible check of the executable model, not a proof about Hybrid.

The observation restriction is essential even when physical access is
race-free. A lock around a live snapshot or insertion flag would still
permit the two negative answers above. Likewise, an idle region cannot
be treated as joined: blocked dependencies remain outstanding work.

The conclusion is **retain the narrow observation boundary for the next
experiment**, not that monotone regions are ready to ship. The model fixes
all task instructions and fact operands in advance. It does not admit
data-dependent task creation, arbitrary pure computations, multiple cells,
or a real dependency-propagation workload. No useful-workload performance,
memory measurement, fairness, deadlock freedom, or actual compiler/runtime
admission result is claimed.

Next, a separate compiler experiment must map a documented `.hy` subset
to these transitions, reject unsupported effects without fallback, and
show that emitted Go uses synchronized storage and enforces all-task join
before snapshot. Then compare a real dependency workload with mutex/actor
baselines, including contention, memory growth, and unsupported cases.
If useful programs require a forbidden observation or unknown effect, narrow
or stop that design rather than weaken the guarantee. Nothing here changes
the existing ownership checker or `GUARANTEES.md` claims.

Prior art remains [LVars](https://doi.org/10.1145/2535838.2535842), as credited
by #482. This experiment claims neither the invention of monotone shared
state nor acceptance into the Wildcard ledger. Selection, hosted CI, final
maintainer review, and any award remain separate decisions.
