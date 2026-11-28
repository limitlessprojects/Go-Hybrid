# Spike `W-1963`: Proof-Producing `hy-check` Sendability Verifier (`#1963`)

Separates ownership/sendability discovery (`hy-check`) from independent justification by replaying source-bound sendability derivations (`RULE_PRIM_COPY`, `RULE_VALUE_STRUCT_CLOSURE`, `RULE_UNIQUE_FRESH_MOVE`) against a cryptographic source/import closure.

## Artifacts
- `spikes/w1963-proof-producing-sendability/sendability_verifier.py`: Independent proof-carrying sendability verifier.
- `spikes/w1963-proof-producing-sendability/tests/test_w1963_sendability_verifier.py`: 5 deterministic unit tests covering positive proofs and forged-certificate attacks.
