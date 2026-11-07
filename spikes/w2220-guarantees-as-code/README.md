# W-2220 — GUARANTEES as code: executable claims and a demotion gate

**Proposal:** [#2220](https://github.com/limitlessprojects/Go-Hybrid/issues/2220) · **Status:** M1 + M2 spike, self-contained, no production files touched.

The repo already landed two halves of the claims-table idea:

- **#710** (`tools/check_guarantees.py`, #2001): `guarantees.json` is the source of the table, and structural checks bind rows to rule ids, fixtures and the rendered doc.
- **#2221** (`tools/check_claims_links.py`): every evidence link and corpus verdict marker is checked.

This spike adds the part neither half has: **a claim is only as strong as the verifier the gate actually executes, and a claim that stops executing demotes itself instead of surviving as prose.**

## M1 — the gate (rows 1, 8, 16)

1. **Typed, executable verifiers.** `demotion_policy.json` names a verifier per row from the closed set `corpus_pair | run_gate | runtime_pin | argument`; M2 adds `test_suite | benchmark`.
2. **Demotion state machine.** `executable → prose-only (owner + expiry + debt ticket) → demoted to no claim`. A guaranteed row may not be prose-only; an expired prose-only row renders as **demoted**, never as a live claim. Rows without a policy entry render as `unregistered` — silence is not a claim.
3. **Rendered table with drift detection.** `--render` emits `rendered_claims.md`; the gate fails when the committed render differs.
4. **Seeded-mutation self-test.** `--self-test` mutates the registry/policy in eight ways and fails unless **all eight** are caught. A gate that cannot fail is not a gate.

## M2 — all 20 rows + coverage

M2 applies the production registry to **every row** without editing production files: `m2_overrides.json` adds only the spike-local negative-falsifier fixtures that `guarantees.json` does not have yet (rows 14/15). It adds two verifier kinds and a generated coverage table:

- `test_suite` — run a focused suite (rows 18/19: the `hyrt` handle-table tests) and check static invariants (row 18: `panic = "abort"`).
- `benchmark` — run `hyb bench-overhead` and require its own `VERDICT: PASS` plus the measured fast-path number (row 20).
- `--render` also writes `coverage.md`: row-by-row state, the verifier, why that verifier (or why not executable), and owner/expiry/debt. The gate fails on coverage-table drift too.

**M2 result (2026-10-31): 16/20 executable, 4/20 prose-only, 0 demoted, wall NOT HIT.** Full run in [RESULTS-M2.md](RESULTS-M2.md).

### Coverage at a glance

| Rows | Verifier | Evidence |
|------|----------|----------|
| 1–7, 10, 11 | `corpus_pair` | Production corpus fixtures already pinned by `guarantees.json` |
| 8 | `run_gate` | `tools/managed_gate.py` baseline + injected-new-block rejection |
| 14, 15 | `corpus_pair` | Spike-local falsifiers: `unsafe` rejected; `import "C"` rejected |
| 16 | `runtime_pin` | Checker accepts; emitted Go panics `send on closed channel` under `-race` |
| 18, 19 | `test_suite` | `hyrt` tests (generation tags / stale handles / copy protocol) + `panic = "abort"` |
| 20 | `benchmark` | `hyb bench-overhead` PASS against the 5 ns budget |
| 9, 12, 13, 17 | `argument` | Prose-only with owner, expiry and debt; see `coverage.md` for why |

## Run it

```bash
# from the repository root, with `cargo build -p hyb` done and Go on PATH
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --timing
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --render

# optional explicit toolchains
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py \
    --hyb compiler/target/debug/hyb --go go

python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py' -v
```

Exit codes: `0` all executable rows verified and both renders match; `1` a violation or a missed self-test mutation; `2` CLI misuse.

## Files

| File | Role |
|------|------|
| `guarantees_gate.py` | verifier runner, demotion state machine, renderer, coverage table, self-test |
| `demotion_policy.json` | per-row status/verifier/owner/expiry/debt/reason (all 20 rows) |
| `m2_overrides.json` | M2-only fixture overlay (rows 14/15); production registry untouched |
| `fixtures/` | spike-local negative-falsifier programs (unsafe / cgo) |
| `rendered_claims.md` | generated claim table (drift-checked) |
| `coverage.md` | generated M2 coverage table (drift-checked) |
| `test_guarantees_gate.py` | 12 unit/integration tests |
| `RESULTS.md` | recorded M1 run on the pinned toolchains |
| `RESULTS-M2.md` | recorded M2 run, coverage, CI cost, falsifier verdict |

## Falsifier (pre-registered with the proposal)

M1 fails if any of these holds:

1. row 1, 8 or 16 has no executable verifier, or its verifier no longer detects the shape it pins;
2. fewer than **5/5** seeded mutations are caught;
3. an expired prose row still renders as a claim;
4. a guaranteed row can be demoted to prose without failing the gate.

M2 fails if:

5. **12 or more of 20** rows are not executable (the whole-idea wall);
6. any row has no policy entry (silence is not a claim);
7. either rendered table drifts from the registry.

Current status: **3/3 M1 rows executed, 8/8 mutations caught, 16/20 rows executable (wall NOT HIT), 12/12 tests green.**

## Honest limits (M3)

- Rows 14/15 use spike-local fixtures; a production migration would move them into `tests/checker-corpus` and update the census.
- Debt tickets are local JSON entries, not GitHub issues; expiry is evaluated at run time, not scheduled.
- The coverage table is spike-local; `GUARANTEES.md` is still generated by `tools/check_guarantees.py`, not by this gate.
- CI/Makefile wiring and the production registry migration are not done.
- Row 9's `-race` callback falsifier is the main missing negative.
- "Executable" is not uniformly strong: rows 18/19 are test-suite evidence (not proof), and row 20 is environment-dependent.
- No prototype is claimed beyond this gate. If the approach cannot express 12 of 20 rows as executable checks without new language features, the wall is the published outcome.
