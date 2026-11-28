# W-4729: Directional Ownership-Breaking Release Witness Synthesizer

## Summary

This spike implements the pilot proposed in `#4729`: given two revisions of a checked API (`old` and `candidate`) with explicit single-value ownership effects (`borrow`, `consume`, `unknown`) and a pinned toolchain, it synthesizes the **shortest** straight-line downstream `.hy` client that compiles against `old` and fails against `candidate`.

## Why Directionality Matters

An undirected signature diff treats `borrow -> consume` and `consume -> borrow` symmetrically. In reality:
- **`borrow -> consume`** shrinks the set of valid callers and breaks existing clients (for example `Submit(x); Inspect(x)`).
- **`consume -> borrow`** relaxes the contract and admits strictly more clients (`0` breaking sequences).
- **Alpha-equivalent local renames** leave the public ownership contract unchanged (`no_witness`).
- **Unsupported aliasing / escaping effects (`unknown`)** return `unknown` instead of fabricating a false witness.

## Implementation

- `spikes/w4729-ownership-breaking-witness/ownership_witness.py`:
  - `find_shortest_breaking_witness(old, candidate)` performs breadth-first search over `(old_state, new_state)` pairs, emits a `.hy` client via `emit_hy_client`, and replays `check_hy_client` on both revisions with the pinned toolchain before reporting `breaking_change`.
  - `enumerate_breaking_sequences(old, candidate, max_len=6)` exhaustively verifies all `127` sequences of length `0..6`.
- `spikes/w4729-ownership-breaking-witness/tests/test_ownership_witness.py`:
  - Verifies the shortest witness `("Submit", "Inspect")` for `borrow -> consume`.
  - Reproduces the exact exhaustive counts (`127` total sequences of length `0..6`: `114` breaking for `borrow -> consume`, `0` for `consume -> borrow`, `0` for unchanged).
  - Verifies negative controls: renamed locals (`no_witness`), unsupported aliasing (`unknown`), and unpinned toolchain (`unknown`).

## Verification

Run:

```bash
python3 -m unittest discover -s spikes/w4729-ownership-breaking-witness/tests -v
```

All 8 unit and exhaustive enumeration tests pass.
