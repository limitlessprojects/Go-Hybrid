# benchmarks/workloads — the three-language workload triplet

The matched-shape control that PLAN row 18 asks for. The community
benchmark report (v0.1.0) compared Go / Rust / Hybrid workload programs
with UNMATCHED queue shapes and measured an 8× "alloc gap"; its own
matched-shape control dissolved the gap into parity. This directory makes
the matched shape the default instead of a manual follow-up.

## How the shapes are shared

`shape.py` is the single source of shape truth. It EMITS all four programs:

| file | language | built by |
|---|---|---|
| `go/main.go` | Go | `go build` in the runtime module (uses `hybench`) |
| `rust/src/main.rs` | Rust | `cargo build --release` (std only) |
| `hy/load.hy` | Hybrid | `hyb check` + `hyb build` (the checker runs) |
| `hy/alloc.hy` | Hybrid | `hyb check` + `hyb build` (the checker runs) |

The generator also owns `rust/Cargo.toml`, including its release profile.

Every twin gets the SAME constants (100 windows, 2000 load items, 32
batches × 128 items × 960 appended bytes) and the SAME loop structure, so a
"gap" measured between twins is a property of the runtimes, not of a
hand-edited harness. Edit shapes ONLY in `shape.py`, then regenerate:

```bash
python3 shape.py        # regenerate the four programs and Rust manifest
python3 shape.py --check  # read-only: exit 1 on changed or missing outputs
```

## Timer honesty (the row's other half)

One timer per WINDOW, never per call: a per-call `time.Now()` costs
~20-30 ns, which would dominate a 3 ns crossing and inflate every
percentile. Each sample is a window mean (ns per item), which is what a
fixed-rate consumer actually sees. The runtime's `runtime/go/hybench`
package carries the HDR-style histogram used by `hyb bench-overhead` and
the Go twin; the Rust and Hybrid twins compute the same nearest-rank
percentiles from the same window means inline.

## Running

```bash
./run.sh
```

Output: one `workload p50=Xns p99=Yns mean=Zns n=N` line per twin, side by
side. The Hybrid twins are also checker exercises at realistic scale — a
workload that stops checking is a red build, not a benchmark.
