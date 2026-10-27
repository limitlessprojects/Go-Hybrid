# Spike `W-1929`: Close-Right Confinement via Linear `CloseRight` Capability (`#1929`)

Separates linear `CloseRight` ownership from channel endpoint (`SendEnd` / `RecvEnd`) multiplicity so `GUARANTEES.md` row 16 (send-on-closed panic freedom) holds with zero runtime cost while preserving MPSC fan-in (Corpus 15) and request/ack bidirectional pipelines (Corpus 16).

## Artifacts
- `spikes/w1929-close-right-confinement/close_capability_checker.py`: Linear `CloseRight` capability verifier.
- `spikes/w1929-close-right-confinement/tests/test_w1929_close_capability_checker.py`: 5 deterministic unit tests.
