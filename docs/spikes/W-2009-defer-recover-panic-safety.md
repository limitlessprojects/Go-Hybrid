# Spike `W-2009`: Same-Goroutine `recover()` & Defer Panic-Safety Checker (`#2009`)

Proves why exit-channel `defer` lowering breaks Go's same-goroutine `recover()` semantics (`recover()` returns `nil` when executed on a consumer goroutine) and implements the 6 static panic-safety rules directly on the AST/CFG walk without relocating `defer`.

## Artifacts
- `spikes/w2009-defer-recover-panic-safety/defer_recover_checker.py`: Same-goroutine `defer`/`recover()` verifier & operational simulator.
- `spikes/w2009-defer-recover-panic-safety/tests/test_w2009_defer_recover_checker.py`: 5 deterministic unit tests covering all 6 rules.
