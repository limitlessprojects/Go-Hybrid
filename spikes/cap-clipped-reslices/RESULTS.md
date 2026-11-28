# Checker-planned capacity clipping — implementation of #1927

This implements the existing proposal by the author of
https://github.com/limitlessprojects/Go-Hybrid/issues/1927. It is not a new
finding, a new Wildcard proposal, or a claim to that proposal's award.

## Pre-registered protocol (before implementation measurements)

Baseline: upstream main `1f0ff978f4deff78c77bca96db0e3e46f32de71b`.
Hypothesis: wrapping a checked two-index slice in an inlined generic helper
can enforce `cap == len`, with no duplicate evaluation, while keeping the
slice workload within 5% of main. Arrays must remain views of the original
storage, not copies. Strings and unresolved bases must remain two-index.
Explicit three-index slices retain their requested capacity.

Functional gates: failing-before/passing-after CLI regressions; checker
plan coverage for slice/array/pointer-to-array, aliases, fields, nested
slices and closures; string/unknown controls; parser and negative checks
for max bounds; evaluation order including `?`; workspace tests, registered
checker corpus, and full differential loop. Do not weaken existing tests.

Performance protocol, fixed before running either variant:

- Build the same `workload.hy` using the main and patched compilers in
  package mode; compile each generated package with the same Go 1.26.9.
- Run `BenchmarkSlice` in `slice_test.go`: dynamic two-index, omitted low,
  omitted high, omitted both, array, pointer-to-array and nested views.
  Each iteration consumes an element and publishes the resulting header,
  so the slice operation cannot be discarded. No append or allocation in
  the measured loop. The array benchmark uses an addressable array field.
- Use native ARM64 toolchains on this host; record actual versions, CPU,
  platform, generated Go, compiler SHAs, commands and unedited outputs.
  No cross-architecture binaries or toolchain-drift override.
- One warmup per variant (excluded). Eleven paired samples, alternating
  main/patched order, `GOMAXPROCS=1`, `-cpu=1`, `-benchtime=500ms`,
  `-count=1`, `-benchmem`. Only one heavy job at a time.
- For each named workload, divide the median patched ns/op by the median
  main ns/op. Every ratio must be <= 1.05; report all samples, medians,
  ratios and allocations. Any failure is a failed gate, not rounded down.
  If an implementation is optimized after a failure, retain failed results
  and rerun the entire protocol as a separately named attempt.
- Timing covers emitted slice workloads, not compiler throughput, appends,
  the FFI bridge, a universal safety proof, or every hardware platform.

Reproduction (from repository root, pinned tools on PATH):

```
python3 spikes/cap-clipped-reslices/bench.py \
  --baseline /path/to/main/hyb --patched /path/to/patched/hyb \
  --out /tmp/cap-clip-bench
```

## Results

Measured on 2026-10-27, native Linux ARM64 (2 CPUs, ARM implementer
`0x41`, part `0xd0c`), Go 1.26.9 and Rust 1.99.0. The protocol above was
committed at `2f5b4cf01de7f5d28a7434b0dd9b50e6495f9692` before either attempt.
Both attempts completed all warmups and all eleven alternating sample pairs.

**Attempt 2 is the final implementation gate.** Compiler source was frozen
at `abc2dbb52fd10ed5244a27c8151127094ae9f5d0` before the build, functional
battery and measurements. Subsequent commits only record results. Compiler
binary SHA-256: `15eb114c8c5e901b6c398f5c1ce722558ec8e575310156bd21a0c3382af83f66`.
The exact source diff was archived locally with SHA-256
`a3752ed9000ad71680d46b90a1547593d3aaee73fe0678923ddd4ce7f2d99c99`;
the source itself is retained in that commit.

| Workload | Main median ns/op | Final median ns/op | Final/main | Attempt 1 ratio |
|---|---:|---:|---:|---:|
| Array | 2.688 | 2.686 | 0.999256 | 1.000000 |
| Nested | 2.688 | 2.631 | 0.978795 | 0.957265 |
| Pointer | 2.688 | 2.688 | 1.000000 | 0.999257 |
| Prefix | 1.853 | 1.849 | 0.997841 | 1.000541 |
| Suffix | 2.353 | 2.185 | 0.928602 | 0.929512 |
| TwoIndex | 2.692 | 2.563 | 0.952080 | 0.952062 |
| Whole | 1.572 | 1.571 | 0.999364 | 0.999365 |

Every final ratio is <= 1.05; the largest is 1.000000. All samples in both
attempts report **0 B/op and 0 allocs/op**. These are measurements of the
seven specified slice workloads on this host, not a claim about all programs
or hardware. Attempt 1 predates the helper namespace correction and is
retained as performance evidence, not as the final correctness gate.

Complete unedited text artifacts (metadata, tool versions, emitted Go, build
commands, warmups, every sample and per-sample JSON) are retained in
[raw/attempt1.txt](raw/attempt1.txt) and [raw/attempt2.txt](raw/attempt2.txt).
No sample, failed measurement attempt, or workload was discarded. There were
no failed timing gates in these two attempts.

### Functional validation of the frozen source

- Main independently accepts the original reslice reproduction and prints
  `1 3`; clipping prints `1 1`. Seven original CLI regressions failed on
  main and passed after implementation. The first implementation's two-file
  regression failed with `hy_clip redeclared`; the final regression generates
  two `.hy` files in one package, runs `go test`, and verifies both capacities.
- The same package regression pins a user-defined `hy_clip`, a user symbol
  matching the first proposed generated name, relative/absolute generation
  stability, and a named Go slice whose method must survive clipping.
- Focused parser/checker/emitter/CLI: **23 passed**. Workspace:
  **1,823 passed**, zero failures. Corpus: **127/127**, managed tripwire
  not fired. Full differential: **41/41 accept-side build+run and 86/86
  reject-side checks**, with no registered verdict changes.
- Initial full differential failed the two bridge builds because this sandbox
  exposes a read-only, non-repository `/tmp/.git`, causing Go VCS stamping to
  return exit 128. The complete loop then passed with `TMPDIR` inside ignored
  `build/verification-tmp` under the real checkout. The compiler was unchanged;
  no check was disabled. Both outputs are retained in the local test log.
- The array-negative fixture uses newline-separated declarations, as required
  by the parser. `int8(128)` folds to `-128` under SPEC X4; main already rejects
  that value in either lo or hi with **HYB-E1002**, not E6002. The new maximum
  pins that same rejection. Positive truncation controls (`int8(256)`,
  `int8(257)`, `int8(258)`) remain accepted. No pre-existing expectation was
  relaxed; the incorrect new E6002 expectation was corrected against main.
- Existing warnings (unused import/dead code and duplicate match arms) remain.
  Hosted CI is pending publication; the required local suites all ran.

To reproduce the functional battery with the pinned native tools on PATH:

```sh
cargo test --manifest-path compiler/Cargo.toml --workspace --locked
cargo build --manifest-path compiler/Cargo.toml --locked -p hyb
python3 tools/corpus_harness.py --bin "$CARGO_TARGET_DIR/debug/hyb"
# Place TMPDIR under the real checkout if /tmp has sandbox Git placeholders.
mkdir -p build/verification-tmp
TMPDIR="$PWD/build/verification-tmp" python3 tools/differential_loop.py \
  --bin "$CARGO_TARGET_DIR/debug/hyb"
```

For the paired benchmark, build the baseline compiler from upstream
`1f0ff978f4deff78c77bca96db0e3e46f32de71b` in a separate target directory,
build the patched compiler from this branch, then run the command in the
pre-registered protocol above with those two binaries. Use a new output
directory for each attempt.
