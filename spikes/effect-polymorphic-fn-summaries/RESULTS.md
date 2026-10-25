# Wildcard Spike — Effect-Polymorphic Function Summaries (Tarjan SCC Fixpoint + Callback Multiplicity)

## Pre-registered hypotheses & falsifiers
- **H1 (Kill `loans.rs` AST body re-walking & `depth <= 4` horizon):** Bottom-up Tarjan-SCC fixpoint inference computes exact parameter-store and spawn summaries at depth `100` and across mutual recursion (`ping <-> pong`) in `< 5 ms`, closing the `depth > 4` soundness gap (`#1698`) by construction.
- **H2 (Flip `GUARANTEES.md` Rows 5 & 9 without Go stdlib body analysis):** Adding a 2-bit callback invocation multiplicity (`1`, `sync*`, `async*`) to `TypeExpr::Fn` and curated Go callback signatures (`sort.Slice` = `sync*`, `http.HandleFunc` / `time.AfterFunc` = `async*`) admits sequential mutating closures (`sort.Slice`, `0` races under `go run -race`) without `managed` while rejecting concurrent callback escapes (`HYB-E3002`).
- **Falsifier F1:** Stored callbacks (`struct handler { cb fn() }` escaping into heap containers) cannot be discharged by call-site multiplicity alone; they must unify the `fn` type's effect row to `async*` at assignment/field-init time (`O4.9`).

## Measured results
- `python3 -m unittest discover -s spikes/effect-polymorphic-fn-summaries/tests` — **2/2 PASS** (`102`-node call graph + mutual-recursion SCC fixpoint converged in `< 1 ms`; `go run -race` oracle confirmed `0` races on `sync*` `sort.Slice` mutating capture).
