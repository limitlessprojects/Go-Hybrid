# spikes/w012-wasi-driver — the stock checker as an in-process WASI module

Wildcard **W-012** (issue #268) asks whether the validator can run inside
`gopls`/`go vet` instead of as a subprocess. The proposal names two engines:
a self-hosted Go rewrite, or `hy-check.wasm` in `wazero` behind a custom ABI.

This spike measures the cheap endcap nobody has costed: **the binary we
already ship, compiled to `wasm32-wasip1`, run as a WASI command module** —
no new crate, no export surface, no second contract to keep in sync.

- Pre-registration and gates: [PREREG.md](PREREG.md)
- Method, numbers, and what they mean: [RESULTS.md](RESULTS.md)
- Harness (throwaway per SPEC §5): `work/`

## Reproduce

```sh
# same tree, both binaries
(cd compiler && cargo build --release -p hyb)
(cd compiler && cargo build --release -p hyb --target wasm32-wasip1)

cd spikes/w012-wasi-driver/work
go test ./...        # harness tests; the engine comparison runs only with env set

HYBRID_NATIVE_PATH=$PWD/../../../compiler/target/release/hyb \
HYBRID_WASM_PATH=$PWD/../../../compiler/target/wasm32-wasip1/release/hyb.wasm \
go test -run TestEnginesAgreeOnFixtures -v ./

go run . parity -wasm $HYBRID_WASM_PATH -hyb $HYBRID_NATIVE_PATH \
    -corpus ../../../tests/checker-corpus -out parity.json
go run . bench  -wasm $HYBRID_WASM_PATH -hyb $HYBRID_NATIVE_PATH \
    -corpus ../../../tests/checker-corpus -files 25 -edits 100 -reps 3 -out bench.json
```

`parity` prints one line per corpus program that disagrees and a summary;
`bench` prints per-engine percentiles, the invocation floor, and per-repetition
rows, and writes the full report (including every parity row) as JSON.

Requires `github.com/tetratelabs/wazero` (the only dependency, in this
spike's own module — the repository is untouched).
