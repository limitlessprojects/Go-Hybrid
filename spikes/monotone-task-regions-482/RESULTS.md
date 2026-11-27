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

Pending. This section will record commands and measured output after the
preregistration commit.
