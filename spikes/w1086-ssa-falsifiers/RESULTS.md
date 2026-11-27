# #1086: pre-registered dominance-only falsifiers

Task: https://github.com/limitlessprojects/Go-Hybrid/issues/1086

Base: `cc59624818a7e2298e3b7d977762600192cbaf77` (main).

## Scope and thresholds (registered before execution)

This independent companion experiment tests the proposal's strongest P1
claim: single assignment and ordinary CFG dominance alone can replace
ownership back-edge/alias checks. It does not implement a Hybrid frontend,
replace Tier-0, claim corpus equivalence for a new checker, or award itself
a ledger entry. No production compiler behavior will change.

- F1: a single-definition, dominance-valid loop that consumes the same
  resource on two iterations is a counterexample. A bounded interpreter must
  report the second consumption; a fresh allocation on every iteration is
  the safe control. A structural dominance rejection of either graph does
  not count as a counterexample.
- F2: a dominance-valid alias copy followed by a move and an alias write is
  a counterexample. A deep-copy control must stay independent. This tests
  storage identity, not just the spelling of lexical identifiers.
- F3: a dominance-valid SSA map update with a live old map version must
  preserve the old version to count as functional. A local Go test will
  compare an in-place helper against a copying helper. Observing the update
  through the old version falsifies in-place map_put as a functional update.
- V1: include negative controls for malformed SSA (non-dominating use,
  duplicate definitions, incorrect phi predecessor sets, and invalid edge
  operands). The verifier must reject these rather than accept everything.
- V2: run the unchanged compiler workspace, full checker corpus, and
  differential harness with the repository's pinned toolchains. Record
  actual counts/exits; these are baseline regressions, not proof that this
  experiment is a replacement compiler. Run the experiment in CI too.

P1 is falsified if any of F1/F2 passes structural SSA verification while
its bounded execution violates ownership. This is a necessary-condition
experiment: one counterexample suffices; it cannot establish the converse.
If P1 fails, do not claim P2 or full historical-corpus parity. The Go map
probe is a semantic check, not an escape-analysis or performance measurement.

## Results

Not run at pre-registration. Results and limitations will be appended after
execution without changing the thresholds above.
