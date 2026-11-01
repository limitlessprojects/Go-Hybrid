# Spike `W-1942`: Fuel-Metered Compute Capsules with a Closed Call Graph (`#1942`)

Implements an opt-in compute capsule with a closed call graph, non-renewable logical fuel budget, explicit bounded call frames, pre-transition checked debit (`>= 1` unit per transition + variable-size byte-slice pre-charging), and fail-closed admission against ambient Go/FFI/task/channel/managed escapes.

## Artifacts
- `spikes/w1942-fuel-metered-compute-capsule/fuel_capsule.py`: Capsule admission checker and fuel-metered state-machine executor.
- `spikes/w1942-fuel-metered-compute-capsule/tests/test_w1942_fuel_capsule.py`: 5 deterministic unit tests.
