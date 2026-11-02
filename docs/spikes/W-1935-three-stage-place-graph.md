# Spike `W-1935`: Three-Stage Place-Graph Lowering (`#1935`)

Implements place-graph lowering in three independently provable stages:
1. **Stage 1:** Pure syntax wrapper (`Expr::Paren`) normalization.
2. **Stage 2:** Canonical `Place`/`Proj` projection lowering (`Field`, `SliceIndex`, `MapIndex`), catching `#1775` (`&m[k].field` / `&m[k][i]` map-slot sub-projection addressability) and `#1822` (receiver auto-borrow on moved roots).
3. **Stage 3:** Fine-grained closure capture places (`CapturePlace(root, proj_path)`), eliminating whole-struct capture false positives when closures touch disjoint fields.

## Artifacts
- `spikes/w1935-three-stage-place-graph/place_graph_stages.py`: Three-stage place-graph normalizer and verifier.
- `spikes/w1935-three-stage-place-graph/tests/test_w1935_place_graph_stages.py`: 4 deterministic unit tests.
