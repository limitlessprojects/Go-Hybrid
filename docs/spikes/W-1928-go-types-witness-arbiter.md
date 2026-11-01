# Spike `W-1928`: `go/types` Witness Arbitrating the Shape Oracle (`#1928`)

Implements a serialized `TypeWitness` mapping `(file, byte_span) -> go/types.Type` that arbitrates `kind_of_expr` in `hy-check` (`Unknown` recovery + disagreement diagnostics for `#1805`/`#1822`) and shares the exact resolved type table with `hy-emit`.

## Artifacts
- `spikes/w1928-go-types-witness-arbiter/type_witness_arbiter.py`: `TypeWitness` arbiter and emitter/checker parity verifier.
- `spikes/w1928-go-types-witness-arbiter/tests/test_w1928_type_witness_arbiter.py`: 5 deterministic unit tests covering all pre-registered criteria.
