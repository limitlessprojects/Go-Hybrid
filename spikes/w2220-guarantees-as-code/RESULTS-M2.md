# RESULTS — W-2220 M2 (run 2026-10-31 UTC)

**Tree:** `spikes/w2220-guarantees-as-code` @ `cb972fb` + M2 changes · **Toolchains:** go1.26.9, rust 1.99.0 · linux/amd64 · checker `compiler/target/debug/hyb`.

M2 applies the production registry (`guarantees.json`) to **all 20 rows** through a spike-local overlay (`m2_overrides.json`) and publishes the coverage table. Production files are untouched.

## 1. The gate, all 20 rows

```
$ python3 spikes/w2220-guarantees-as-code/guarantees_gate.py

  row 1  corpus_pair                        OK  3 fixture(s) agree
  row 2  corpus_pair                        OK  4 fixture(s) agree
  row 3  corpus_pair                        OK  3 fixture(s) agree
  row 4  corpus_pair                        OK  2 fixture(s) agree
  row 5  corpus_pair                        OK  3 fixture(s) agree
  row 6  corpus_pair                        OK  3 fixture(s) agree
  row 7  corpus_pair                        OK  6 fixture(s) agree
  row 8  run_gate(managed_gate)             OK  baseline 2 block(s) pass; injected block rejected
  row 9  argument                           PROSE-ONLY
  row 10 corpus_pair                        OK  3 fixture(s) agree
  row 11 corpus_pair                        OK  2 fixture(s) agree
  row 12 argument                           PROSE-ONLY
  row 13 argument                           PROSE-ONLY
  row 14 corpus_pair                        OK  1 fixture(s) agree
  row 15 corpus_pair                        OK  1 fixture(s) agree
  row 16 runtime_pin(go run -race, expect=panic: send on closed channel) OK  checker accepts; go run -race: panic: send on closed channel
  row 17 argument                           PROSE-ONLY
  row 18 test_suite(cargo test --manifest-path runtime/rust/hyrt/Cargo.toml --quiet) OK  `cargo test ...` exit 0; 1 static invariant(s)
  row 19 test_suite(cargo test --manifest-path runtime/rust/hyrt/Cargo.toml --quiet) OK  `cargo test ...` exit 0
  row 20 benchmark({hyb} bench-overhead, expect=VERDICT: PASS) OK  bench verdict PASS; fast path 3.34 ns ≤ 5.0 ns
guarantees-as-code M2: PASS (16/20 executable, 4/20 prose-only; wall NOT HIT)
```

## 2. Coverage

`coverage.md` is the generated table. Summary:

| State | Rows | Count |
|-------|------|-------|
| executable | 1–8, 10, 11, 14–16, 18–20 | **16/20** |
| prose-only (owner + expiry + debt) | 9, 12, 13, 17 | **4/20** |
| demoted / invalid / unregistered | — | **0/20** |
| pre-registered wall (≥12 not executable) | — | **NOT HIT** (4 not executable) |

### What became executable in M2

| Rows | Verifier | Evidence executed |
|------|----------|-------------------|
| 1–7, 10, 11 | `corpus_pair` | The production corpus fixtures already pinned by `guarantees.json`: the checker's verdict (accept / accept-managed / reject) must agree with each row. |
| 8 | `run_gate` | `tools/managed_gate.py` passes the committed baseline **and** rejects an injected new managed block. |
| 14, 15 | `corpus_pair` | New spike-local negative falsifiers: `unsafe` is a reserved-keyword parse error; `import "C"` is rejected by the curated allowlist. |
| 16 | `runtime_pin` | The checker accepts send-on-closed; the emitted Go panics `send on closed channel` under `go run -race`. |
| 18, 19 | `test_suite` | `cargo test` on `runtime/rust/hyrt` (4 tests: generation tags, stale handles, short-buffer copy protocol) plus the `panic = "abort"` profile invariant for row 18. |
| 20 | `benchmark` | `hyb bench-overhead` emits `VERDICT: PASS` and its measured fast-path number against the 5 ns budget. |

### What could not be executed, and why

| Row | Why not | Debt |
|-----|---------|------|
| 9 — values escaping into Go APIs | The A5 escape is accepted by design. The issue's negative falsifier is a callback that leaks a shared map and is caught only by `-race`; the existing handler smoke client is sequential, so M2 did not build a program that actually races. | `W2220-DEBT-009`, expires 2026-12-31 |
| 12 — `context.Context` cancellation | Cancellation semantics are not modeled; the checker has no surface to observe. | `W2220-DEBT-012`, expires 2026-12-31 |
| 13 — finalizers | No `.hy` surface at all; nothing for a verifier to execute. | `W2220-DEBT-013`, expires 2026-12-31 |
| 17 — Go-side memory safety | Inherited from Go; there is no Hybrid claim to falsify. Go's own toolchain / race detector is the evidence. | `W2220-DEBT-017`, expires 2026-12-31 |

## 3. Seeded-mutation self-test

```
$ python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
  PASS  row 1: flipped fixture verdict is rejected
  PASS  policy drop: row 1 is not left unregistered
  PASS  guaranteed prose-only: refused
  PASS  runtime pin: wrong panic text is rejected
  PASS  prose expiry: rendered row is demoted
  PASS  test_suite: a failing command is rejected
  PASS  benchmark: a wrong verdict is rejected
  PASS  M2 coverage: a dropped row is caught
self-test: 8/8 mutations caught
```

## 4. Tests

```
$ python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py' -v
Ran 12 tests in 3.295s
OK
```

## 5. CI cost (measured on this machine, warm caches)

`--timing` prints the per-row cost:

```
  row 1  corpus_pair ... [0.009s]
  ...
  row 8  run_gate(managed_gate) ... [0.065s]
  row 16 runtime_pin ... [0.117s]
  row 18 test_suite ... [0.021s]
  row 19 test_suite ... [0.021s]
  row 20 benchmark ... [3.035s]
gate wall time: 3.342s
```

| Step | Wall time |
|------|-----------|
| `--render` (both tables) | 0.045 s |
| gate, all 20 rows | 3.342 s |
| `--self-test` (8 mutations) | 3.314 s |
| unit tests (12) | 3.214 s |
| `cargo test` on `hyrt` (warm) | 0.022 s (cold build ≈ 4.0 s) |
| `hyb bench-overhead` (warm) | 3.048 s |

The gate does **not** build the compiler; CI already builds `hyb` in the compiler job. The dominant new cost is the benchmark (~3 s, 91% of the gate) and, on a cold runner, the `hyrt` test build (~4 s). The corpus checks and managed gate duplicate existing CI steps, so the net new CI time is roughly **7–10 s warm**; production wiring could replace the separate corpus/managed steps with the gate rather than add to them.

## 6. Falsifier verdict

The pre-registered whole-idea falsifier was: **if 12 or more of 20 rows cannot be expressed as executable checks without new language features, the approach is a documented wall.**

M2 result: **4/20 rows are not executable** (rows 9, 12, 13, 17). The wall is **NOT HIT**; the approach survives M2 on its own terms.

The honest caveat: "executable" is not uniformly strong.

- Rows 1–7, 10, 11, 14, 15 are corpus-verdict checks — they fail if the checker's verdict drifts.
- Row 8 is a gate check — it fails if a new managed block appears unreviewed.
- Row 16 is a negative runtime pin — it fails if the panic text or checker acceptance changes.
- Rows 18/19 are test-suite evidence, not a proof; language-level memory safety is inherited from rustc.
- Row 20 is environment-dependent: a slow CI runner can exceed the 5 ns budget and turn the gate red. That is the claim being enforced, but it is also the flakiest verifier in the set.

## 7. Honest limits / M3

- Rows 14/15 use **spike-local** fixtures. A production migration would move them into `tests/checker-corpus` and update the corpus census.
- Debt tickets are local JSON entries, not GitHub issues; expiry is evaluated at run time, not scheduled.
- `coverage.md` and `rendered_claims.md` are spike-local; `GUARANTEES.md` is still generated by `tools/check_guarantees.py`, not by this gate.
- CI/Makefile wiring and the production registry migration are not done.
- Row 9's `-race` callback falsifier is the main missing negative and the first M3 item.
- No production files were touched.

## Reproduction

```bash
cargo build -p hyb
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --render
python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py'
```
