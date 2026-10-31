# RESULTS — W-2220 M1 (run 2026-10-30 UTC)

**Tree:** `main` @ `bd4f03a` · **Toolchains:** go1.26.9, rust 1.99.0 · linux/amd64 · checker `compiler/target/debug/hyb`.

## 1. The gate

```
$ python3 spikes/w2220-guarantees-as-code/guarantees_gate.py

  row 1  corpus_pair                        OK  3 fixture(s) agree
  row 8  run_gate(managed_gate)             OK  baseline 2 block(s) pass; injected block rejected
  row 9  argument                           PROSE-ONLY
  row 16 runtime_pin(go run -race, expect=panic: send on closed channel) OK  checker accepts; go run -race: panic: send on closed channel
  ...
guarantees-as-code M1: PASS (all executable rows verified; rendered table matches)
```

Rows 2–7, 10–15 and 17–20 render as `unregistered (M1 out of scope)`. Row 9 is the prose-only demo: no claim, executes nothing, expires 2026-12-31, debt `W2220-DEBT-009`.

## 2. Row 16 — the negative falsifier

```
$ hyb check pin.hy
pin.hy: OK (0 diagnostics)          # the checker makes no protocol claim

$ hyb build pin.hy -o pin --emit-go
emitted pin.go

$ go run -race pin.go
panic: send on closed channel       # exit 1
```

The claim is "no claim"; the pin makes that absence observable instead of asserted.

## 3. Row 8 — run_gate catches a new hatch

```
$ python3 tools/managed_gate.py hyb gates/managed-baseline.json \
      tests/checker-corpus/02-shared-map-managed.hy \
      tests/checker-corpus/18-pool-shared-struct-managed.hy
managed gate: PASS (2 block(s), baseline 2)          # exit 0

$ python3 tools/managed_gate.py hyb gates/managed-baseline.json /tmp/injected.hy
NEW MANAGED BLOCK — review required: /tmp/injected.hy:70 [mutex] ...
managed gate: FAILED — ...                            # exit 1
```

## 4. Seeded-mutation self-test

```
$ python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test

  PASS  row 1: flipped fixture verdict is rejected
  PASS  policy drop: row 1 is not left unregistered
  PASS  guaranteed prose-only: refused
  PASS  runtime pin: wrong panic text is rejected
  PASS  prose expiry: rendered row is demoted
self-test: 5/5 mutations caught
```

## 5. Tests

```
$ python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py' -v
Ran 8 tests in 0.111s
OK
```

## Reproduction

```bash
cargo build -p hyb
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py'
```

## Interpretation against the pre-registered falsifier

| Falsifier | Result |
|-----------|--------|
| any of rows 1/8/16 lacks an executable verifier | **not hit** — 3/3 executed |
| fewer than 5/5 seeded mutations caught | **not hit** — 5/5 |
| expired prose still renders as a claim | **not hit** — renders as demoted |
| guaranteed row demotable to prose silently | **not hit** — structural failure |

M1 succeeds on its own terms; rows 2–20, the `differential` verifier, GitHub-issue debt tickets, and CI/Makefile wiring remain M2.
