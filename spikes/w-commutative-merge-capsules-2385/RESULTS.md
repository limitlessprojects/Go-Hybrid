# W-2385 Spike Results — Certified Commutative Merge Capsules (`merge[M]`)

**Proposal:** #2385 | **Author:** `@ket4min-susan` | **Verifiers:** `python3 spikes/w-commutative-merge-capsules-2385/merge_capsules_verifier.py --check --live-go` & `python3 -m unittest discover -s spikes/w-commutative-merge-capsules-2385/tests -p "test_*.py" -v`

## 1. Pre-Registered Hypotheses & Declared Falsifiers (`P1`–`P3`, `P4a`, `P4b`, `P4c`)

| ID | Gate | Measured Result | Verdict |
| :--- | :--- | :--- | :--- |
| **P1** | Task-local `Shard[R, M]` split (`Live -> Moved`) + pairwise tree `Settle()` with 0 locks/atomics/channels; negative `go build` rejects post-`Finish` / post-`Settle` UAM | 8 workers × 10,000 updates = `80,000` (`sampled=320`), 0 races; both negative programs rejected by `go build` | **PASS** |
| **P2** | 3-element symbolic/probe check (`HYB-E3401`) accepts commutative monoids (`Histogram`) and rejects order-dependent reducers (`append`) | `HistogramReducer` accepted; `NonCommutativeLogReducer` rejected | **PASS** |
| **P3** | Contended 8-worker fan-out aggregation throughput (`work/merge_test.go`) | `MergeCapsule`: **`11,699 ns/op`** vs `SharedAtomic`: `157,080 ns/op` (**13.4x faster**), `Mutex`: `379,947 ns/op` (**32.5x faster**), `Channel`: `1,088,463 ns/op` (**93.0x faster**); hot-loop `0.97 ns/op`, `0 B/op`, `0 allocs/op` | **PASS** |
| **P4a** | Falsifier #1: Non-commutative reducer under permutations `[0,1,2,3]` vs `[3,1,0,2]` | Produces `[w0,w1,w2,w3] != [w3,w1,w0,w2]` -> `HYB-E3401` mandatory | **FIRED** |
| **P4b** | Falsifier #2: Interior reference alias escaping `s.finish()` without `O5.2` freeze | Shallow-adopted map mutated from `10` to `9999` after `Settle` -> `O5.2` deep freeze (`HYB-E3003`) mandatory | **FIRED** |
| **P4c** | Falsifier #3: IEEE-754 `float64` addition across left- vs right-leaning reduction trees | `(1e16 + -1e16) + 1.0 = 1.0 != 0.0 = 1e16 + (-1e16 + 1.0)` -> floating-point types rejected by `HYB-E3401` | **FIRED** |
