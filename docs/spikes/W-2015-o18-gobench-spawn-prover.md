# Spike `W-2015`: O18 GoBench Spawn Census & O4.0 Closure/Method-Value Prover (`#2015`)

Reproduces the 35-kernel GoBench GoKer spawn census (`73` total spawns: `4` plain `fn`, `17` method-value, `52` closure; `69/73 = 94.5%` unprovable under baseline O4.0 `HYB-E3005`, `30/34` spawn-bearing kernels with 0 provable spawns) and evaluates O4.0 extension (method-value receiver sendability + immediately-invoked closure capture lifting) across the 7 single-spawn kernels and `etcd/4876`.

## Artifacts
- `spikes/w2015-o18-gobench-spawn-prover/gobench_spawn_prover.py`: Census verifier + O4.0 closure/method-value lift analyzer.
- `spikes/w2015-o18-gobench-spawn-prover/tests/test_w2015_gobench_spawn_prover.py`: 4 deterministic unit tests.
